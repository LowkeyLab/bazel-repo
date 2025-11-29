import { Component, effect, signal } from '@angular/core';
import { CommonModule } from '@angular/common';

interface Game {
  id: string;
  // Add more fields as needed
}

@Component({
  selector: 'mindreadr-games',
  standalone: true,
  imports: [CommonModule],
  templateUrl: './games.component.html',
})
export class GamesComponent {
  games = signal<Game[]>([
    { id: 'ABCD1234' },
    { id: 'EFGH5678' },
    // Replace with real data from a service
  ]);

  joinGame(id: string) {
    // Implement join logic or navigation
    alert(`Joining game ${id}`);
  }
}
