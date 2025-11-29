import { Component, signal, OnInit } from '@angular/core';
import { CommonModule } from '@angular/common';
import { HttpClient } from '@angular/common/http';
import { environment } from '../../environments/environment';

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

  constructor(private http: HttpClient) {}

  ngOnInit() {
    this.fetchGamesCount();
  }

  fetchGamesCount() {
    this.loading.set(true);
    this.error.set(null);

    const url = environment.API_BASE_URL === '/' ? '/games' : `${environment.API_BASE_URL}/games`;

    this.http.get<GamesResponse>(url).subscribe({
      next: (response) => {
        this.count.set(response.count);
        this.loading.set(false);
      },
      error: (err) => {
        this.error.set(err?.message ?? 'Failed to fetch games count');
        this.loading.set(false);
      },
    });
  }
}
