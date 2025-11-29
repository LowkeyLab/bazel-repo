import { Component, signal } from '@angular/core';
import { CommonModule } from '@angular/common';
import { GameService, Game } from '../services/game.service';

@Component({
  selector: 'mindreadr-landing',
  standalone: true,
  imports: [CommonModule],
  templateUrl: './landing.component.html',
})
export class LandingComponent {
  creating = signal(false);
  lastGame = signal<Game | null>(null);
  error = signal<string | null>(null);
  showJoinModal = signal(false);
  ws: WebSocket | null = null;

  constructor(private games: GameService) {}

  createGame() {
    this.error.set(null);
    this.creating.set(true);
    this.games.createGame().subscribe({
      next: (game) => {
        this.lastGame.set(game);
        this.creating.set(false);
        this.showJoinModal.set(true);
      },
      error: (err) => {
        this.error.set(err?.message ?? 'Failed to create game');
        this.creating.set(false);
      },
    });
  }

  joinGame() {
    const game = this.lastGame();
    if (!game) return;
    // Use relative URL so Angular proxy upgrades WS: /games/{id}/live
    const protocol = location.protocol === 'https:' ? 'wss' : 'ws';
    const host = location.host; // use current dev server host; proxy forwards to backend
    const url = `${protocol}://${host}/games/${game.id}/live`;
    try {
      this.ws?.close();
      this.ws = new WebSocket(url);
      this.ws.onopen = () => {
        // Close modal when connected
        this.showJoinModal.set(false);
      };
      this.ws.onmessage = (evt) => {
        // For now, log server messages; future work could update UI/state
        // eslint-disable-next-line no-console
        console.log('WS message:', evt.data);
      };
      this.ws.onerror = () => {
        this.error.set('WebSocket connection error');
      };
      this.ws.onclose = (e) => {
        // eslint-disable-next-line no-console
        console.log('WS closed', e.code, e.reason);
      };
    } catch (e: any) {
      this.error.set(e?.message ?? 'Failed to open WebSocket');
    }
  }
}
