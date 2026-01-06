import { HttpClient } from '@angular/common/http';
import { Injectable, inject } from '@angular/core';
import { Observable } from 'rxjs';
import { environment } from '../../environments/environment';
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
}
