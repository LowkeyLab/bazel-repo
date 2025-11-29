import { Component, signal, OnInit } from '@angular/core';
import { CommonModule } from '@angular/common';
import { GameService } from '../services/game.service';
import { forkJoin } from 'rxjs';

interface GamesResponse {
  count: number;
}

@Component({
  selector: 'mindreadr-games-count',
  standalone: true,
  imports: [CommonModule],
  templateUrl: './games-count.component.html',
  styleUrls: ['./games-count.component.css'],
})
export class GamesCountComponent implements OnInit {
  count = signal<number | null>(null);
  openCount = signal<number | null>(null);
  completedCount = signal<number | null>(null);
  loading = signal(true);
  error = signal<string | null>(null);

  constructor(private games: GameService) {}

  ngOnInit() {
    this.fetchCounts();
  }

  fetchCounts() {
    this.loading.set(true);
    this.error.set(null);
    forkJoin({
      active: this.games.getGamesCount(),
      open: this.games.getOpenGamesCount(),
      completed: this.games.getCompletedGamesCount(),
    }).subscribe({
      next: ({ active, open, completed }) => {
        this.count.set(active);
        this.openCount.set(open);
        this.completedCount.set(completed);
        this.loading.set(false);
      },
      error: (err) => {
        this.error.set(err?.message ?? 'Failed to fetch game stats');
        this.loading.set(false);
      },
    });
  }
}
