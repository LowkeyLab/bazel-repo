import {
  Component,
  OnDestroy,
  OnInit,
  inject,
  input,
  signal,
} from '@angular/core';

import { ActivatedRoute, Router } from '@angular/router';
import { GameDto } from '../services/game.types';
import confetti from 'canvas-confetti';
import { GameRoundsComponent } from '../game-rounds/game-rounds.component';

@Component({
  selector: 'mindreadr-game-summary',
  standalone: true,
  imports: [GameRoundsComponent],
  templateUrl: './game-summary.component.html',
  styleUrls: ['./game-summary.component.css'],
})
export class GameSummaryComponent implements OnInit, OnDestroy {
  private readonly route = inject(ActivatedRoute);
  private readonly router = inject(Router);

  // Configurable animation delay (milliseconds between each round appearing)
  animationDelayMs = input<number>(150);

  gameId = signal<string>('');
  game = signal<GameDto | null>(null);
  error = signal<string | null>(null);
  currentPlayer = signal<any | null>(null);
  private initialPlayerName: string | null = null;

  // No websocket subscriptions for summary view

  ngOnInit(): void {
    // Read navigation extras state if available for faster initial render
    const nav = this.router.currentNavigation
      ? this.router.currentNavigation()
      : null;
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

    // Celebrate with a burst of confetti
    this.fireConfetti();
  }

  ngOnDestroy(): void {
    // No cleanup needed
  }

  roundsCount(): number {
    const g = this.game();
    return g ? g.rounds.length : 0;
  }

  // Determine the guessed word to display in the header.
  // Always uses the last guess from the latest round.
  guessedWord(): string | null {
    const g = this.game();
    if (!g || g.rounds.length === 0) return null;
    const latest = g.rounds[g.rounds.length - 1];
    const entries = Object.values(latest.guesses ?? {});
    if (entries.length === 0) return null;
    const last = entries[entries.length - 1];
    return String(last);
  }

  sorted_player_names_from_players(players: GameDto['players']): string[] {
    return players
      .map((p) => p.name)
      .filter((n) => n.length > 0)
      .sort((a, b) => a.localeCompare(b));
  }

  backToGames(): void {
    this.router.navigate(['/games']);
  }

  getPlayerName(p: any): string {
    // Prefer initial navigation-provided name for stability
    if (this.initialPlayerName) return this.initialPlayerName;
    return p?.name ?? 'Player';
  }

  // Canvas-based confetti effect
  private fireConfetti(): void {
    try {
      // Simple, reliable burst using canvas-confetti
      confetti({
        particleCount: 140,
        spread: 80,
        startVelocity: 45,
        scalar: 0.9,
        ticks: 200,
        origin: { y: 0.2 },
      });

      // Add a secondary burst from the sides for flair
      confetti({
        particleCount: 80,
        angle: 60,
        spread: 55,
        origin: { x: 0, y: 0.4 },
      });
      confetti({
        particleCount: 80,
        angle: 120,
        spread: 55,
        origin: { x: 1, y: 0.4 },
      });
    } catch {
      // No-op if canvas not available
    }
  }
}
