import { Injectable } from '@angular/core';
import { HttpClient } from '@angular/common/http';
import { Observable } from 'rxjs';
import { environment } from '../../environments/environment';
import { GameDto, GameState } from './game.types';

export interface GameSummary {
  waitingForPlayerGames: number;
  inProgressGames: number;
  completedGames: number;
}

@Injectable({ providedIn: 'root' })
export class GameService {
  constructor(private http: HttpClient) {}

  createGame(): Observable<GameDto> {
    // Use relative path; proxy handles dev. Fallback to environment base for non-dev builds.
    const url = `${environment.API_BASE_URL}/games`;
    return this.http.post<GameDto>(url, {});
  }

  /**
   * Fetches aggregated game counts from the backend.
   */
  getSummary(): Observable<GameSummary> {
    const url = `${environment.API_BASE_URL}/games/summary`;
    return this.http.get<GameSummary>(url);
  }

  /**
   * Fetches games filtered by backend status enum.
   * Example: /games?status=WAITING_FOR_PLAYERS
   */
  getGamesByStatus(status: GameState): Observable<GameDto[]> {
    const url = `${environment.API_BASE_URL}/games?status=${encodeURIComponent(status)}`;
    return this.http.get<GameDto[]>(url);
  }
}
