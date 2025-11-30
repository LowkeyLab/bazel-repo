import { Component, OnDestroy, OnInit, inject, signal } from '@angular/core';
import { CommonModule } from '@angular/common';
import { ActivatedRoute, Router } from '@angular/router';
import { GameWsService, GameDto, RoundDto } from '../services/game-ws.service';

@Component({
  selector: 'mindreadr-game-summary',
  standalone: true,
  imports: [CommonModule],
  templateUrl: './game-summary.component.html',
})
export class GameSummaryComponent implements OnInit, OnDestroy {
  private readonly route = inject(ActivatedRoute);
  private readonly ws = inject(GameWsService);
  private readonly router = inject(Router);

  gameId = signal<string>('');
  game = signal<GameDto | null>(null);
  error = signal<string | null>(null);
  currentPlayer = signal<any | null>(null);
  private initialPlayerName: string | null = null;

  private connection: ReturnType<GameWsService['connect']> | null = null;
  private subs: Array<{ unsubscribe: () => void }> = [];

  ngOnInit(): void {
    // Read navigation extras state if available for faster initial render
    const nav = this.router.getCurrentNavigation();
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
    this.connection = this.ws.connect(id);
    const conn = this.connection;
    this.subs.push(
      conn.gameState$.subscribe((g) => this.game.set(g)),
      conn.errors$.subscribe((e) => this.error.set(e)),
      conn.terminated$.subscribe(() => {}),
      conn.playerJoined$.subscribe((player) => this.currentPlayer.set(player)),
    );
  }

  ngOnDestroy(): void {
    try {
      this.subs.forEach((s) => s.unsubscribe());
    } catch {}
    if (this.connection) this.connection.close();
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
