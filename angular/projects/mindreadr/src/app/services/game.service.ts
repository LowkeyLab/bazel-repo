import { Injectable } from '@angular/core';
import { HttpClient } from '@angular/common/http';
import { Observable } from 'rxjs';
import { environment } from '../../environments/environment';

export interface Game {
  id: string;
  [key: string]: unknown;
}

@Injectable({ providedIn: 'root' })
export class GameService {
  constructor(private http: HttpClient) {}

  createGame(): Observable<Game> {
    // Use relative path; proxy handles dev. Fallback to environment base for non-dev builds.
    const url = environment.API_BASE_URL === '/' ? '/games' : `${environment.API_BASE_URL}/games`;
    return this.http.post<Game>(url, {});
  }
}
