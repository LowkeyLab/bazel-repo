import { HttpClient } from '@angular/common/http';
import { Injectable, inject } from '@angular/core';
import { Observable, timer } from 'rxjs';
import { switchMap, takeWhile, map } from 'rxjs/operators';
import { environment } from '../../environments/environment';
import { DEFAULT_POLL_INTERVAL_MS } from '../config/polling';
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
  private readonly apiUrl = `${environment.apiUrl}/protected`;

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
   * Polls contest details at a fixed interval until the contest is closed or resolved.
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
      map((contest) => {
        // Compute totals by option for convenience
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
      }),
      takeWhile(
        (contest) =>
          contest.status !== 'CLOSED' && contest.status !== 'RESOLVED',
        true,
      ),
    );
  }
}
