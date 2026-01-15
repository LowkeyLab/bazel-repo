import { HttpClient } from '@angular/common/http';
import { Injectable, inject, NgZone } from '@angular/core';
import { Observable, timer } from 'rxjs';
import { switchMap, takeWhile, map } from 'rxjs/operators';
import { environment } from '../../environments/environment';
import { DEFAULT_POLL_INTERVAL_MS } from '../config/polling';
import { AuthService } from './auth.service';
import type {
  Contest,
  CreateContestRequest,
  MakePredictionRequest,
  PayoutBreakdown,
  ResolveContestRequest,
} from '../models/contest.model';

@Injectable({
  providedIn: 'root',
})
export class ContestService {
  private readonly http = inject(HttpClient);
  private readonly auth = inject(AuthService);
  private readonly zone = inject(NgZone);
  private readonly apiUrl = `${environment.apiUrl}/protected`;

  private mapContestWithTotals(contest: Contest): Contest & {
    totals: { byOption: Map<number, { clout: number; count: number }> };
  } {
    const totals = new Map<number, { clout: number; count: number }>();
    for (const option of contest.options) {
      const predictions = contest.predictions.filter(
        (p) => p.option_id === option.id,
      );
      totals.set(option.id, {
        clout: predictions.reduce((sum, p) => sum + p.clout, 0),
        count: predictions.length,
      });
    }
    return { ...contest, totals: { byOption: totals } };
  }

  createContest(request: CreateContestRequest): Observable<Contest> {
    const { circle_id, ...rest } = request;
    return this.http.post<Contest>(
      `${this.apiUrl}/circles/${circle_id}/contests`,
      rest,
    );
  }

  getContest(circleId: number, contestId: number): Observable<Contest> {
    return this.http.get<Contest>(
      `${this.apiUrl}/circles/${circleId}/contests/${contestId}`,
    );
  }

  makePrediction(
    circleId: number,
    contestId: number,
    request: MakePredictionRequest,
  ): Observable<void> {
    return this.http.post<void>(
      `${this.apiUrl}/circles/${circleId}/contests/${contestId}/predictions`,
      request,
    );
  }

  lockContest(circleId: number, contestId: number): Observable<void> {
    return this.http.post<void>(
      `${this.apiUrl}/circles/${circleId}/contests/${contestId}/lock`,
      {},
    );
  }

  resolveContest(
    circleId: number,
    contestId: number,
    request: ResolveContestRequest,
  ): Observable<void> {
    return this.http.post<void>(
      `${this.apiUrl}/circles/${circleId}/contests/${contestId}/resolve-distribute`,
      request,
    );
  }

  getPayoutBreakdown(
    circleId: number,
    contestId: number,
  ): Observable<PayoutBreakdown> {
    return this.http.get<PayoutBreakdown>(
      `${this.apiUrl}/circles/${circleId}/contests/${contestId}/payout-breakdown`,
    );
  }

  /**
   * Polls contest details at a fixed interval until the contest is expired or resolved.
   * Maps the contest to include computed totals for convenience.
   *
   * @param circleId - Circle ID
   * @param contestId - Contest ID
   * @param intervalMs - Polling interval in milliseconds (default: DEFAULT_POLL_INTERVAL_MS)
   * @returns Observable that emits contest details with totals until contest reaches a terminal status
   */
  pollContestDetails(
    circleId: number,
    contestId: number,
    intervalMs: number = DEFAULT_POLL_INTERVAL_MS,
  ): Observable<
    Contest & {
      totals: { byOption: Map<number, { clout: number; count: number }> };
    }
  > {
    return timer(0, intervalMs).pipe(
      switchMap(() => this.getContest(circleId, contestId)),
      map((contest) => this.mapContestWithTotals(contest)),
      takeWhile(
        (contest) =>
          contest.status !== 'EXPIRED' && contest.status !== 'RESOLVED',
        true,
      ),
    );
  }

  streamContestDetails(
    circleId: number,
    contestId: number,
  ): Observable<
    Contest & {
      totals: { byOption: Map<number, { clout: number; count: number }> };
    }
  > {
    return new Observable((observer) => {
      const token = this.auth.token();
      if (!token) {
        observer.error('No auth token');
        return;
      }

      const url = `${this.apiUrl}/circles/${circleId}/contests/${contestId}/stream?token=${token}`;
      const eventSource = new EventSource(url);

      const fetchAndEmit = () => {
        this.getContest(circleId, contestId).subscribe({
          next: (contest) => {
            observer.next(this.mapContestWithTotals(contest));
            if (contest.status === 'EXPIRED' || contest.status === 'RESOLVED') {
              eventSource.close();
              observer.complete();
            }
          },
          error: (err) => {
            console.error('Failed to fetch updated contest', err);
          },
        });
      };

      eventSource.onopen = () => {
        // Initial fetch
        this.zone.run(() => fetchAndEmit());
      };

      eventSource.addEventListener('contest_updated', () => {
        this.zone.run(() => fetchAndEmit());
      });

      eventSource.onerror = (error) => {
        this.zone.run(() => {
          if (eventSource.readyState === EventSource.CLOSED) {
            // If closed unexpectedly, maybe try to reconnect or error out
            // For now, let's error if we can't connect
            observer.error(error);
          }
        });
      };

      return () => {
        eventSource.close();
      };
    });
  }
}
