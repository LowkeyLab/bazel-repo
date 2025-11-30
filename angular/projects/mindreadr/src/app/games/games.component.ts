import { Component, OnInit, inject, signal } from '@angular/core';
import { CommonModule } from '@angular/common';
import { GameService, Game } from '../services/game.service';

@Component({
  selector: 'mindreadr-games',
  standalone: true,
  imports: [CommonModule],
  templateUrl: './games.component.html',
})
export class GamesComponent implements OnInit {
  private readonly gameService = inject(GameService);

  // Signal holding currently open (waiting for players) games.
  games = signal<Game[]>([]);
  loading = signal<boolean>(false);
  error = signal<string | null>(null);

  ngOnInit(): void {
    this.fetchWaitingGames();
  }

  fetchWaitingGames() {
    this.loading.set(true);
    this.error.set(null);
    this.gameService.getGamesByStatus('WAITING_FOR_PLAYERS').subscribe({
      next: (games) => {
        this.games.set(games);
        this.loading.set(false);
      },
      error: (err) => {
        this.error.set('Failed to load games');
        this.loading.set(false);
        // In a real app, log via a logging service
        console.error('Error fetching games', err);
      },
    });
  }

  createNewGame() {
    // Optimistically disable actions
    this.loading.set(true);
    this.error.set(null);
    this.gameService.createGame().subscribe({
      next: (game) => {
        // Refresh the list of open games; newly created games should appear
        this.fetchWaitingGames();
        // Optional: surface the ID for now
        console.info('Created game', game.id);
      },
      error: (err) => {
        this.error.set('Failed to create game');
        this.loading.set(false);
        console.error('Error creating game', err);
      },
    });
  }

  joinGame(id: string) {
    alert(`Joining game ${id}`);
  }
}
