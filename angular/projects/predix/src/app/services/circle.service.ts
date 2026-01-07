import { HttpClient } from '@angular/common/http';
import { Injectable, inject } from '@angular/core';
import { Observable, timer } from 'rxjs';
import { switchMap } from 'rxjs/operators';
import { environment } from '../../environments/environment';
import type {
  Circle,
  CreateCircleRequest,
  AddMemberRequest,
} from '../models/circle.model';
import type { Contest } from '../models/contest.model';

@Injectable({
  providedIn: 'root',
})
export class CircleService {
  private readonly http = inject(HttpClient);
  private readonly apiUrl = `${environment.apiUrl}/protected/circles`;

  createCircle(request: CreateCircleRequest): Observable<Circle> {
    return this.http.post<Circle>(this.apiUrl, request);
  }

  listUserCircles(): Observable<Circle[]> {
    return this.http.get<Circle[]>(this.apiUrl);
  }

  getCircle(id: number): Observable<Circle> {
    return this.http.get<Circle>(`${this.apiUrl}/${id}`);
  }

  getCircleContests(id: number): Observable<Contest[]> {
    return this.http.get<Contest[]>(`${this.apiUrl}/${id}/contests`);
  }

  pollCircleContests(
    id: number,
    interval: number = 5000,
  ): Observable<Contest[]> {
    return timer(0, interval).pipe(switchMap(() => this.getCircleContests(id)));
  }

  addMember(circleId: number, request: AddMemberRequest): Observable<void> {
    return this.http.post<void>(`${this.apiUrl}/${circleId}/members`, request);
  }

  joinCircle(circleId: number): Observable<void> {
    return this.http.post<void>(`${this.apiUrl}/${circleId}/join`, {});
  }
}
