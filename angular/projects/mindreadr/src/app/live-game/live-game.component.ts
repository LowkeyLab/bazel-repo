import { Component, OnDestroy, OnInit, inject, signal } from '@angular/core';
import { CommonModule } from '@angular/common';
import { FormsModule } from '@angular/forms';
import { StatusBadgeComponent } from './status-badge.component';
import { ActivatedRoute, Router } from '@angular/router';
import { GameWsService } from '../services/game-ws.service';
import { GameDto, RoundDto, Player } from '../services/game.types';

interface Toast {
  message: string;
  type: 'info' | 'success' | 'error';
}

@Component({
  selector: 'mindreadr-live-game',
  standalone: true,
  imports: [CommonModule, FormsModule, StatusBadgeComponent],
  templateUrl: './live-game.component.html',
})
export class LiveGameComponent implements OnInit, OnDestroy {
  private readonly route = inject(ActivatedRoute);
  private readonly ws = inject(GameWsService);
  private readonly router = inject(Router);

  gameId = signal<string>('');
  game = signal<GameDto | null>(null);
  error = signal<string | null>(null);
  terminated = signal<string | null>(null);
  guess = signal<string>('');
  currentPlayer = signal<any | null>(null);

  toasts = signal<Toast[]>([]);

  private connection: ReturnType<GameWsService['connect']> | null = null;
  private subs: Array<{ unsubscribe: () => void }> = [];
  private previousPlayerKeys: string[] = [];
  private initialGameStateProcessed = false;

  ngOnInit(): void {
    const id = this.route.snapshot.paramMap.get('id');
    if (!id) {
      this.error.set('Missing game id');
      return;
    }
    this.gameId.set(id);
    const conn = (this.connection = this.ws.connect(id));

    this.subs.push(
      conn.gameState$.subscribe((g) => {
        const prevKeys = this.previousPlayerKeys;
        const newKeys = g.players.map((p) => this.getPlayerIdentityKey(p));

        // Detect newly added players after the initial state.
        if (this.initialGameStateProcessed) {
          const newlyAdded = newKeys.filter((k) => !prevKeys.includes(k));
          if (newlyAdded.length > 0) {
            // Show a toast for each newly added player.
            g.players.forEach((p) => {
              const key = this.getPlayerIdentityKey(p);
              if (newlyAdded.includes(key)) {
                this.showToast(
                  `Player joined: ${this.getPlayerName(p)}`,
                  'info',
                );
              }
            });
          }
        } else {
          // Skip toasts for initial full state.
          this.initialGameStateProcessed = true;
        }

        this.previousPlayerKeys = newKeys;
        this.game.set(g);

        // Redirect to summary when game completed
        if (g.state === 'COMPLETED') {
          this.router.navigate([`/games/${this.gameId()}/summary`], {
            state: {
              playerName: this.getPlayerName(this.currentPlayer()),
              finalGame: g,
            },
          });
        }
        // Redirect to timeout page when game terminated due to round limit
        if (g.state === 'TERMINATED' && this.hasReachedRoundLimit(g)) {
          this.router.navigate([`/games/${this.gameId()}/timeout`], {
            state: {
              playerName: this.getPlayerName(this.currentPlayer()),
              finalGame: g,
            },
          });
        }
      }),
      conn.errors$.subscribe((e) => this.error.set(e)),
      conn.terminated$.subscribe((reason) => {
        this.terminated.set(reason);
        if (reason === 'COMPLETED') {
          const g = this.game();
          this.router.navigate([`/games/${this.gameId()}/summary`], {
            state: {
              playerName: this.getPlayerName(this.currentPlayer()),
              finalGame: g ?? undefined,
            },
          });
        }
        if (reason === 'TERMINATED') {
          const g = this.game();
          if (g && this.hasReachedRoundLimit(g)) {
            this.router.navigate([`/games/${this.gameId()}/timeout`], {
              state: {
                playerName: this.getPlayerName(this.currentPlayer()),
                finalGame: g ?? undefined,
              },
            });
          }
        }
      }),
      conn.playerJoined$.subscribe((player) => {
        // Still capture current player but do not emit a toast here.
        this.currentPlayer.set(player);
      }),
      conn.playerLeft$.subscribe((player) => {
        this.showToast(`Player left: ${this.getPlayerName(player)}`, 'info');
        // Remove player from previous list so a rejoin later can trigger toast.
        const key = this.getPlayerIdentityKey(player);
        this.previousPlayerKeys = this.previousPlayerKeys.filter(
          (k) => k !== key,
        );
      }),
    );
  }
  showToast(message: string, type: 'info' | 'success' | 'error' = 'info') {
    const toast: Toast = { message, type };
    this.toasts.update((arr) => [...arr, toast]);
    setTimeout(() => {
      this.toasts.update((arr) => arr.filter((t) => t !== toast));
    }, 3500);
  }

  /** Attempt to build a stable identity key for a player for diffing purposes. */
  private getPlayerIdentityKey(player: Player): string {
    // Use player name as the identity key since Player only has name property
    return `name:${player.name}`;
  }

  submitGuess() {
    const value = this.guess().trim();
    if (!value || !this.connection) return;
    this.connection.submitGuess(value);
    this.guess.set('');
  }

  leaveGame() {
    if (this.connection) this.connection.close();
    this.router.navigate(['/games']);
  }

  getPlayerName(p: Player | null | undefined): string {
    return p?.name ?? 'Player';
  }

  objectKeys<T extends object>(obj: T): Array<keyof T & string> {
    return Object.keys(obj) as Array<keyof T & string>;
  }

  /** Get rounds in reverse order (most recent first) */
  getReversedRounds(rounds: RoundDto[]): RoundDto[] {
    return [...rounds].reverse();
  }

  getStatusLabel(state: string): string {
    switch (state) {
      case 'WAITING_FOR_PLAYERS':
        return 'Waiting for Players';
      case 'IN_PROGRESS':
        return 'In Progress';
      case 'COMPLETED':
        return 'Completed';
      default:
        return state;
    }
  }

  /** Map game state to status type string */
  getStatusType(
    state: string,
  ): 'success' | 'warning' | 'info' | 'error' | 'neutral' {
    switch (state) {
      case 'WAITING_FOR_PLAYERS':
        return 'warning';
      case 'IN_PROGRESS':
        return 'info';
      case 'COMPLETED':
        return 'success';
      case 'TERMINATED':
        return 'error';
      default:
        return 'neutral';
    }
  }

  /** Map termination state to status type */
  getTerminationType(state: string): 'success' | 'error' | 'neutral' {
    switch (state) {
      case 'COMPLETED':
        return 'success';
      case 'TERMINATED':
        return 'error';
      default:
        return 'neutral';
    }
  }

  /** Latest (active) round or null */
  private latestRound(): RoundDto | null {
    const g = this.game();
    if (!g || g.rounds.length === 0) return null;
    return g.rounds[g.rounds.length - 1];
  }

  /** Whether a round has guesses from all currently present players */
  isRoundComplete(round: RoundDto, game: GameDto): boolean {
    return (
      Object.keys(round.guesses).length >= game.players.length &&
      game.players.length > 0
    );
  }

  /** Attempt to detect if the given player has already guessed in this round. */
  hasPlayerGuessed(round: RoundDto, player: Player): boolean {
    if (!round || !player) return false;
    return Object.keys(round.guesses).includes(player.name);
  }

  /** True if current player is waiting for others (already guessed, round incomplete). */
  waitingForOtherPlayer(): boolean {
    const g = this.game();
    const p = this.currentPlayer();
    const r = this.latestRound();
    if (!g || !p || !r) return false;
    return this.hasPlayerGuessed(r, p) && !this.isRoundComplete(r, g);
  }

  /** Hide guesses until every player has submitted for that round. */
  shouldHideGuesses(round: RoundDto, game: GameDto): boolean {
    return !this.isRoundComplete(round, game);
  }

  /** Check if game has reached the round limit */
  hasReachedRoundLimit(game: GameDto): boolean {
    if (!game || game.rounds.length === 0) return false;
    const lastRound = game.rounds[game.rounds.length - 1];
    return lastRound.number >= game.roundLimit;
  }

  /** Calculate rounds remaining */
  getRoundsRemaining(game: GameDto): number {
    const currentRound =
      game.rounds.length > 0 ? game.rounds[game.rounds.length - 1].number : 0;
    return Math.max(0, game.roundLimit - currentRound);
  }

  /** Get color class based on rounds remaining */
  getRoundsRemainingColor(game: GameDto): string {
    const remaining = this.getRoundsRemaining(game);
    const total = game.roundLimit;
    const percentage = (remaining / total) * 100;

    if (percentage > 60) {
      return 'text-success'; // Green
    } else if (percentage > 30) {
      return 'text-warning'; // Yellow/Orange
    } else {
      return 'text-error'; // Red
    }
  }

  ngOnDestroy(): void {
    try {
      this.subs.forEach((s) => s.unsubscribe());
    } catch {}
    if (this.connection) this.connection.close();
  }
}
