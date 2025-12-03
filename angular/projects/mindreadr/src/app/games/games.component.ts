import { Component, OnInit, OnDestroy, inject, signal } from '@angular/core';

import { GameService } from '../services/game.service';
import { GameDto } from '../services/game.types';
import { Router } from '@angular/router';
import { Subscription, interval } from 'rxjs';
import { switchMap, catchError } from 'rxjs/operators';
import { EMPTY } from 'rxjs';

/** Polling interval for refreshing the games list (in milliseconds) */
const POLLING_INTERVAL_MS = 1000;

@Component({
  selector: 'mindreadr-games',
  standalone: true,
  imports: [],
  templateUrl: './games.component.html',
})
export class GamesComponent implements OnInit, OnDestroy {
  private readonly gameService = inject(GameService);
  private readonly router = inject(Router);
  private pollingSubscription: Subscription | null = null;

  // Signal holding currently open (waiting for players) games.
  games = signal<GameDto[]>([]);
  loading = signal<boolean>(false);
  error = signal<string | null>(null);

  ngOnInit(): void {
    this.fetchWaitingGames();
    this.startPolling();
  }

  ngOnDestroy(): void {
    this.stopPolling();
  }

  private startPolling(): void {
    this.pollingSubscription = interval(POLLING_INTERVAL_MS)
      .pipe(
        switchMap(() =>
          this.gameService.getGamesByStatus('WAITING_FOR_PLAYERS').pipe(
            catchError((err) => {
              // Silently ignore refresh errors to avoid flickering
              console.error('Error refreshing games', err);
              return EMPTY;
            }),
          ),
        ),
      )
      .subscribe((games) => {
        this.games.set(games);
      });
  }

  private stopPolling(): void {
    if (this.pollingSubscription) {
      this.pollingSubscription.unsubscribe();
      this.pollingSubscription = null;
    }
  }

  fetchWaitingGames() {
    this.loading.set(true);
    this.error.set(null);
    this.gameService.getGamesByStatus('WAITING_FOR_PLAYERS').subscribe({
      next: (games) => {
        this.games.set(games);
        this.loading.set(false);
      },
      error: (err) => {
        this.error.set('Failed to load games');
        this.loading.set(false);
        // In a real app, log via a logging service
        console.error('Error fetching games', err);
      },
    });
  }

  createNewGame() {
    // Optimistically disable actions
    this.loading.set(true);
    this.error.set(null);
    this.gameService.createGame().subscribe({
      next: (game) => {
        // Refresh the list of open games; newly created games should appear
        this.fetchWaitingGames();
        // Optional: surface the ID for now
        console.info('Created game', game.id);
      },
      error: (err) => {
        this.error.set('Failed to create game');
        this.loading.set(false);
        console.error('Error creating game', err);
      },
    });
  }

  joinGame(id: string) {
    this.router.navigate(['/games', id, 'live']);
  }
}
