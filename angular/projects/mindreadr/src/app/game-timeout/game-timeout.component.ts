import { Component, OnDestroy, OnInit, inject, signal } from '@angular/core';
import { CommonModule } from '@angular/common';
import { ActivatedRoute, Router } from '@angular/router';
import { GameDto, RoundDto } from '../services/game.types';

@Component({
  selector: 'mindreadr-game-timeout',
  standalone: true,
  imports: [CommonModule],
  templateUrl: './game-timeout.component.html',
})
export class GameTimeoutComponent implements OnInit, OnDestroy {
  private readonly route = inject(ActivatedRoute);
  private readonly router = inject(Router);

  gameId = signal<string>('');
  game = signal<GameDto | null>(null);
  error = signal<string | null>(null);
  currentPlayer = signal<any | null>(null);
  private initialPlayerName: string | null = null;

  // No websocket subscriptions for timeout view

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
      this.error.set('Game data unavailable. Redirecting to games...');
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

  sortedRounds(rounds: GameDto['rounds']): GameDto['rounds'] {
    return (rounds ?? [])
      .slice()
      .sort((a: RoundDto, b: RoundDto) => a.number - b.number);
  }
  sorted_player_names_from_players(players: GameDto['players']): string[] {
    return players
      .map((p) => p.name)
      .filter((n) => n.length > 0)
      .sort((a, b) => a.localeCompare(b));
  }

  sorted_player_names_for_round(
    game: GameDto,
    round: GameDto['rounds'][number],
  ): string[] {
    const names = this.sorted_player_names_from_players(game?.players ?? []);
    return names.filter(
      (n) =>
        round?.guesses &&
        Object.prototype.hasOwnProperty.call(round.guesses, n),
    );
  }

  tryAgain(): void {
    this.router.navigate(['/games']);
  }

  getPlayerName(p: any): string {
    // Prefer initial navigation-provided name for stability
    if (this.initialPlayerName) return this.initialPlayerName;
    return p?.name ?? 'Player';
  }
}
