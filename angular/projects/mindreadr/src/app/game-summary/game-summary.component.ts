import { Component, OnDestroy, OnInit, inject, signal } from '@angular/core';
import { CommonModule } from '@angular/common';
import { ActivatedRoute, Router } from '@angular/router';
import { GameDto } from '../services/game-ws.service';

@Component({
  selector: 'mindreadr-game-summary',
  standalone: true,
  imports: [CommonModule],
  templateUrl: './game-summary.component.html',
})
export class GameSummaryComponent implements OnInit, OnDestroy {
  private readonly route = inject(ActivatedRoute);
  private readonly router = inject(Router);

  gameId = signal<string>('');
  game = signal<GameDto | null>(null);
  error = signal<string | null>(null);
  currentPlayer = signal<any | null>(null);
  private initialPlayerName: string | null = null;

  // No websocket subscriptions for summary view

  ngOnInit(): void {
    // Read navigation extras state if available for faster initial render
    const nav = this.router.currentNavigation ? this.router.currentNavigation() : null;
    const state: any = nav?.extras?.state ?? history.state ?? {};
    if (state?.playerName) {
      this.initialPlayerName = state.playerName as string;
      this.currentPlayer.set({ name: state.playerName });
    }
    if (state?.finalGame) {
      this.game.set(state.finalGame as GameDto);
    }
    const id = this.route.snapshot.paramMap.get('id');
    if (!id) {
      this.error.set('Missing game id');
      return;
    }
    this.gameId.set(id);

    // If no final game data was provided, show inline message then redirect.
    if (!this.game()) {
      this.error.set('Summary unavailable. Redirecting to games...');
      setTimeout(() => this.router.navigate(['/games']), 1500);
      return;
    }
  }

  ngOnDestroy(): void {
    // Nothing to clean up; no subscriptions.
  }

  roundsCount(): number {
    const g = this.game();
    return g ? g.rounds.length : 0;
  }

  objectKeys<T extends object>(obj: T): Array<keyof T & string> {
    return Object.keys(obj) as Array<keyof T & string>;
  }

  backToGames(): void {
    this.router.navigate(['/games']);
  }

  getPlayerName(p: any): string {
    try {
      // Prefer initial navigation-provided name for stability
      if (this.initialPlayerName) return this.initialPlayerName;
      return p?.name?.name ?? p?.name ?? 'Player';
    } catch {
      return 'Player';
    }
  }
}
