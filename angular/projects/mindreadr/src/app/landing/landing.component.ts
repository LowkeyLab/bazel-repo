import { Component } from '@angular/core';

import { RouterLink } from '@angular/router';
import { GamesCountComponent } from '../games-count/games-count.component';

@Component({
  selector: 'mindreadr-landing',
  standalone: true,
  imports: [RouterLink, GamesCountComponent],
  templateUrl: './landing.component.html',
})
export class LandingComponent {}
