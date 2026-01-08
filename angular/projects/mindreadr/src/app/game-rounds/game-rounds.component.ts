import { Component, OnDestroy, OnInit, input, signal } from '@angular/core';
import { GameDto, RoundDto } from '../services/game.types';

@Component({
  selector: 'mindreadr-game-rounds',
  standalone: true,
  imports: [],
  templateUrl: './game-rounds.component.html',
})
export class GameRoundsComponent implements OnInit, OnDestroy {
  // Inputs
  game = input.required<GameDto | null>();
  animationDelayMs = input<number>(150);

  // Signal for staggered rounds animation
  sortedRounds = signal<GameDto['rounds']>([]);
  private animationTimeouts: number[] = [];

  ngOnInit(): void {
    this.animateRoundsIn();
  }

  ngOnDestroy(): void {
    // Clean up animation timeouts
    this.animationTimeouts.forEach((id) => clearTimeout(id));
    this.animationTimeouts = [];
  }

  private animateRoundsIn(): void {
    const g = this.game();
    if (!g) return;

    const sorted = (g.rounds ?? [])
      .slice()
      .sort((a: RoundDto, b: RoundDto) => a.number - b.number);

    if (sorted.length === 0) {
      this.sortedRounds.set([]);
      return;
    }

    // Start with empty array
    this.sortedRounds.set([]);

    // Add each round with a configurable delay
    const delay = this.animationDelayMs();
    sorted.forEach((round, index) => {
      const timeoutId = window.setTimeout(() => {
        this.sortedRounds.update((current) => [...current, round]);
      }, index * delay);
      this.animationTimeouts.push(timeoutId);
    });
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
}
