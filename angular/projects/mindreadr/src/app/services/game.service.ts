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

export interface GameSummary {
  waitingForPlayerGames: number;
  inProgressGames: number;
  completedGames: number;
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
   * Fetches aggregated game counts from the backend.
   */
  getSummary(): Observable<GameSummary> {
    const url = `${environment.API_BASE_URL}/games/summary`;
    return this.http.get<GameSummary>(url);
  }

  /**
   * Returns the number of games that are currently in progress.
   */
  getGamesCount(): Observable<number> {
    return this.getSummary().pipe(map((s) => s.inProgressGames));
  }

  /**
   * Returns the number of games that are open for joining (waiting for players).
   */
  getOpenGamesCount(): Observable<number> {
    return this.getSummary().pipe(map((s) => s.waitingForPlayerGames));
  }

  /**
   * Returns the number of games that have completed.
   */
  getCompletedGamesCount(): Observable<number> {
    return this.getSummary().pipe(map((s) => s.completedGames));
  }

  /**
   * Fetches games filtered by backend status enum.
   * Example: /games?status=WAITING_FOR_PLAYERS
   */
  getGamesByStatus(status: GameState): Observable<Game[]> {
    const url = `${environment.API_BASE_URL}/games?status=${encodeURIComponent(status)}`;
    return this.http.get<Game[]>(url);
  }
}
