import { Injectable } from '@angular/core';
import { HttpClient } from '@angular/common/http';
import { Observable, map } from 'rxjs';
import { environment } from '../../environments/environment';

type GameState = 'WAITING_FOR_PLAYERS' | 'IN_PROGRESS' | 'COMPLETED';

export interface Game {
  // Backend uses Kotlin value class for id; it is serialized as a string.
  id: string;
  // State enum used to determine open/active/completed
  state?: GameState;
  // Other properties are ignored by the client
  [key: string]: unknown;
}

@Injectable({ providedIn: 'root' })
export class GameService {
  constructor(private http: HttpClient) {}

  createGame(): Observable<Game> {
    // Use relative path; proxy handles dev. Fallback to environment base for non-dev builds.
    const url = `${environment.API_BASE_URL}/games`;
    return this.http.post<Game>(url, {});
  }

  /**
   * Returns the number of games that are currently in progress.
   */
  getGamesCount(): Observable<number> {
    const url = `${environment.API_BASE_URL}/games`;
    return this.http
      .get<Game[]>(url)
      .pipe(map((games) => games.filter((g) => g.state === 'IN_PROGRESS').length));
  }

  /**
   * Returns the number of games that are open for joining (waiting for players).
   */
  getOpenGamesCount(): Observable<number> {
    const url = `${environment.API_BASE_URL}/games`;
    return this.http
      .get<Game[]>(url)
      .pipe(map((games) => games.filter((g) => g.state === 'WAITING_FOR_PLAYERS').length));
  }

  /**
   * Returns the number of games that have completed.
   */
  getCompletedGamesCount(): Observable<number> {
    const url = `${environment.API_BASE_URL}/games`;
    return this.http
      .get<Game[]>(url)
      .pipe(map((games) => games.filter((g) => g.state === 'COMPLETED').length));
  }
}
