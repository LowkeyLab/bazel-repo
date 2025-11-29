import { Component, signal, OnInit } from '@angular/core';
import { CommonModule } from '@angular/common';
import { GameService } from '../services/game.service';

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
  loading = signal(true);
  error = signal<string | null>(null);

  constructor(private games: GameService) {}

  ngOnInit() {
    this.fetchGamesCount();
  }

  fetchGamesCount() {
    this.loading.set(true);
    this.error.set(null);
    this.games.getGamesCount().subscribe({
      next: (count) => {
        this.count.set(count);
        this.loading.set(false);
      },
      error: (err) => {
        this.error.set(err?.message ?? 'Failed to fetch games count');
        this.loading.set(false);
      },
    });
  }
}
