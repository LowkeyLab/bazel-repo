import { Component, OnDestroy, OnInit, inject, signal } from '@angular/core';
import { CommonModule } from '@angular/common';
import { ActivatedRoute, Router } from '@angular/router';
import { GameDto } from '../services/game-ws.service';
import confetti from 'canvas-confetti';

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

    // Celebrate with a burst of confetti
    this.fireConfetti();
  }

  ngOnDestroy(): void {
    // Nothing to clean up; no subscriptions.
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

  // Canvas-based confetti effect
  private fireConfetti(): void {
    try {
      const canvas = document.createElement('canvas');
      canvas.style.position = 'fixed';
      canvas.style.top = '0';
      canvas.style.left = '0';
      canvas.style.width = '100%';
      canvas.style.height = '100%';
      canvas.style.pointerEvents = 'none';
      canvas.style.zIndex = '9999';
      canvas.width = window.innerWidth;
      canvas.height = window.innerHeight;
      document.body.appendChild(canvas);

      const ctx = canvas.getContext('2d');
      if (!ctx) {
        canvas.remove();
        return;
      }

      const colors = ['#FF6B6B', '#FFD93D', '#6BCB77', '#4D96FF', '#B983FF'];
      const particles: Array<{
        x: number;
        y: number;
        vx: number;
        vy: number;
        size: number;
        color: string;
        rotation: number;
        rotationSpeed: number;
        alpha: number;
      }> = [];

      const count = 100;
      const centerX = canvas.width / 2;
      const startY = 50;

      for (let i = 0; i < count; i++) {
        const angle = (Math.random() - 0.5) * Math.PI;
        const velocity = 10 + Math.random() * 8;
        particles.push({
          x: centerX + (Math.random() - 0.5) * 100,
          y: startY,
          vx: Math.cos(angle) * velocity,
          vy: Math.sin(angle) * velocity,
          size: 4 + Math.random() * 6,
          color: colors[Math.floor(Math.random() * colors.length)],
          rotation: Math.random() * Math.PI * 2,
          rotationSpeed: (Math.random() - 0.5) * 0.2,
          alpha: 1,
        });
      }

      const duration = 2000;
      const gravity = 0.5;
      const decay = 0.98;
      const start = performance.now();

      const animate = (t: number) => {
        const elapsed = t - start;
        if (elapsed > duration) {
          canvas.remove();
          return;
        }

        ctx.clearRect(0, 0, canvas.width, canvas.height);

        let alive = 0;
        for (const p of particles) {
          p.vy = p.vy * decay + gravity;
          p.vx *= decay;
          p.x += p.vx;
          p.y += p.vy;
          p.rotation += p.rotationSpeed;
          p.alpha *= 0.985;

          if (p.alpha > 0.05 && p.y < canvas.height + 20) {
            alive++;
            ctx.save();
            ctx.globalAlpha = p.alpha;
            ctx.translate(p.x, p.y);
            ctx.rotate(p.rotation);
            ctx.fillStyle = p.color;
            ctx.fillRect(-p.size / 2, -p.size / 2, p.size, p.size);
            ctx.restore();
          }
        }

        if (alive > 0) {
          requestAnimationFrame(animate);
        } else {
          canvas.remove();
        }
      };

      requestAnimationFrame(animate);
    } catch {
      // No-op if canvas not available
    }
  }
}
