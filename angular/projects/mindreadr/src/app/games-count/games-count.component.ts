import { Component, signal, OnInit, OnDestroy } from '@angular/core';
import { CommonModule } from '@angular/common';
import { GameService } from '../services/game.service';
import { interval, exhaustMap, EMPTY, catchError, Subscription } from 'rxjs';

@Component({
  selector: 'mindreadr-games-count',
  standalone: true,
  imports: [CommonModule],
  templateUrl: './games-count.component.html',
  styleUrls: ['./games-count.component.css'],
})
export class GamesCountComponent implements OnInit, OnDestroy {
  count = signal<number | null>(null);
  openCount = signal<number | null>(null);
  completedCount = signal<number | null>(null);
  loading = signal(true);
  error = signal<string | null>(null);
  private refreshSub?: Subscription;

  constructor(private games: GameService) {}

  ngOnInit() {
    this.fetchCounts();
    this.startRefreshTimer();
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

  private startRefreshTimer() {
    this.refreshSub = interval(1000)
      .pipe(
        exhaustMap(() =>
          this.games.getSummary().pipe(
            catchError((err) => {
              this.error.set(err?.message ?? 'Failed to fetch game stats');
              return EMPTY;
            }),
          ),
        ),
      )
      .subscribe(({ inProgressGames, waitingForPlayerGames, completedGames }) => {
        this.count.set(inProgressGames);
        this.openCount.set(waitingForPlayerGames);
        this.completedCount.set(completedGames);
      });
  }

  ngOnDestroy() {
    this.refreshSub?.unsubscribe();
  }
}
