import { HttpClient } from '@angular/common/http';
import { Injectable, inject } from '@angular/core';
import { Observable } from 'rxjs';
import { environment } from '../../environments/environment';
import type {
  Contest,
  CreateContestRequest,
  MakePredictionRequest,
  ResolveContestRequest,
} from '../models/contest.model';

@Injectable({
  providedIn: 'root',
})
export class ContestService {
  private readonly http = inject(HttpClient);
  private readonly apiUrl = `${environment.apiUrl}/protected/contests`;

  createContest(request: CreateContestRequest): Observable<Contest> {
    return this.http.post<Contest>(this.apiUrl, request);
  }

  getContest(id: number): Observable<Contest> {
    return this.http.get<Contest>(`${this.apiUrl}/${id}`);
  }

  makePrediction(
    contestId: number,
    request: MakePredictionRequest,
  ): Observable<void> {
    return this.http.post<void>(
      `${this.apiUrl}/${contestId}/predictions`,
      request,
    );
  }

  lockContest(contestId: number): Observable<void> {
    return this.http.post<void>(`${this.apiUrl}/${contestId}/lock`, {});
  }

  resolveContest(
    contestId: number,
    request: ResolveContestRequest,
  ): Observable<void> {
    return this.http.post<void>(`${this.apiUrl}/${contestId}/resolve`, request);
  }
}
