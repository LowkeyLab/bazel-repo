import { HttpClient } from '@angular/common/http';
import { Injectable, inject } from '@angular/core';
import { Observable } from 'rxjs';
import type {
  Circle,
  CreateCircleRequest,
  AddMemberRequest,
} from '../models/circle.model';

@Injectable({
  providedIn: 'root',
})
export class CircleService {
  private readonly http = inject(HttpClient);
  private readonly apiUrl = 'http://localhost:8080/circles';

  createCircle(request: CreateCircleRequest): Observable<Circle> {
    return this.http.post<Circle>(this.apiUrl, request);
  }

  getCircle(id: number): Observable<Circle> {
    return this.http.get<Circle>(`${this.apiUrl}/${id}`);
  }

  addMember(circleId: number, request: AddMemberRequest): Observable<void> {
    return this.http.post<void>(`${this.apiUrl}/${circleId}/members`, request);
  }
}
