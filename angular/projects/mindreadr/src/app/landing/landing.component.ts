import { Component } from '@angular/core';
import { CommonModule } from '@angular/common';
import { GamesCountComponent } from '../games-count/games-count.component';

@Component({
  selector: 'mindreadr-landing',
  standalone: true,
  imports: [CommonModule, GamesCountComponent],
  templateUrl: './landing.component.html',
})
export class LandingComponent {}
