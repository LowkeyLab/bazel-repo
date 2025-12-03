import { Component, signal, OnInit } from '@angular/core';

import { GameService } from '../services/game.service';

@Component({
  selector: 'mindreadr-games-count',
  standalone: true,
  imports: [],
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
    this.games.getSummary().subscribe({
      next: ({ inProgressGames, waitingForPlayerGames, completedGames }) => {
        this.count.set(inProgressGames);
        this.openCount.set(waitingForPlayerGames);
        this.completedCount.set(completedGames);
        this.loading.set(false);
      },
      error: (err) => {
        this.error.set(err?.message ?? 'Failed to fetch game stats');
        this.loading.set(false);
      },
    });
  }
}
