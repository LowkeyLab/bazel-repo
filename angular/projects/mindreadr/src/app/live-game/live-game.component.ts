import { Component, OnDestroy, OnInit, inject, signal } from '@angular/core';
import { CommonModule } from '@angular/common';
import { FormsModule } from '@angular/forms';
import { ActivatedRoute, Router } from '@angular/router';
import { GameWsService, GameDto, RoundDto } from '../services/game-ws.service';

interface Toast {
  message: string;
  type: 'info' | 'success' | 'error';
}

@Component({
  selector: 'mindreadr-live-game',
  standalone: true,
  imports: [CommonModule, FormsModule],
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
                this.showToast(`Player joined: ${this.getPlayerName(p)}`, 'info');
              }
            });
          }
        } else {
          // Skip toasts for initial full state.
          this.initialGameStateProcessed = true;
        }

        this.previousPlayerKeys = newKeys;
        this.game.set(g);
      }),
      conn.errors$.subscribe((e) => this.error.set(e)),
      conn.terminated$.subscribe((reason) => this.terminated.set(reason)),
      conn.playerJoined$.subscribe((player) => {
        // Still capture current player but do not emit a toast here.
        this.currentPlayer.set(player);
      }),
      conn.playerLeft$.subscribe((player) => {
        this.showToast(`Player left: ${this.getPlayerName(player)}`, 'info');
        // Remove player from previous list so a rejoin later can trigger toast.
        const key = this.getPlayerIdentityKey(player);
        this.previousPlayerKeys = this.previousPlayerKeys.filter((k) => k !== key);
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
  private getPlayerIdentityKey(player: any): string {
    if (!player) return 'unknown';
    const idFields = [player.id, player.playerId, player.uuid];
    const firstId = idFields.find((v) => v !== undefined && v !== null);
    if (firstId !== undefined) return `id:${firstId}`;
    // Fall back to name.
    return `name:${this.getPlayerName(player)}`;
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

  getPlayerName(p: any): string {
    try {
      return p?.name?.name ?? p?.name ?? 'Player';
    } catch {
      return 'Player';
    }
  }

  objectKeys<T extends object>(obj: T): Array<keyof T & string> {
    return Object.keys(obj) as Array<keyof T & string>;
  }

  getStatusBadgeClass(state: string): string {
    switch (state) {
      case 'WAITING_FOR_PLAYERS':
        return 'bg-yellow-100 text-yellow-700';
      case 'IN_PROGRESS':
        return 'bg-emerald-100 text-emerald-700';
      case 'COMPLETED':
        return 'bg-blue-100 text-blue-700';
      default:
        return 'bg-gray-100 text-gray-700';
    }
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

  /** Latest (active) round or null */
  private latestRound(): RoundDto | null {
    const g = this.game();
    if (!g || g.rounds.length === 0) return null;
    return g.rounds[g.rounds.length - 1];
  }

  /** Whether a round has guesses from all currently present players */
  isRoundComplete(round: RoundDto, game: GameDto): boolean {
    return Object.keys(round.guesses).length >= game.players.length && game.players.length > 0;
  }

  /** Attempt to detect if the given player has already guessed in this round. */
  hasPlayerGuessed(round: RoundDto, player: any): boolean {
    if (!round || !player) return false;
    const name = this.getPlayerName(player);
    const keys = Object.keys(round.guesses);
    if (keys.includes(name)) return true;
    // Fallbacks: try common id fields
    const pid = (player.id ?? player.playerId ?? player.uuid ?? '').toString();
    if (pid && keys.includes(pid)) return true;
    return false;
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

  ngOnDestroy(): void {
    try {
      this.subs.forEach((s) => s.unsubscribe());
    } catch {}
    if (this.connection) this.connection.close();
  }
}
