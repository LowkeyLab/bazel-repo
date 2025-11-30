import { Component, OnDestroy, OnInit, inject, signal } from '@angular/core';
import { CommonModule } from '@angular/common';
import { FormsModule } from '@angular/forms';
import { ActivatedRoute, Router } from '@angular/router';
import { GameWsService, GameDto } from '../services/game-ws.service';

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

  private connection: ReturnType<GameWsService['connect']> | null = null;
  private subs: Array<{ unsubscribe: () => void }> = [];

  ngOnInit(): void {
    const id = this.route.snapshot.paramMap.get('id');
    if (!id) {
      this.error.set('Missing game id');
      return;
    }
    this.gameId.set(id);
    const conn = (this.connection = this.ws.connect(id));

    this.subs.push(
      conn.gameState$.subscribe((g) => this.game.set(g)),
      conn.errors$.subscribe((e) => this.error.set(e)),
      conn.terminated$.subscribe((reason) => this.terminated.set(reason)),
    );
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

  ngOnDestroy(): void {
    try {
      this.subs.forEach((s) => s.unsubscribe());
    } catch {}
    if (this.connection) this.connection.close();
  }
}
