import { Routes } from '@angular/router';
import { LandingComponent } from './landing/landing.component';
import { GamesComponent } from './games/games.component';
import { LiveGameComponent } from './live-game/live-game.component';

export const routes: Routes = [
  { path: '', component: LandingComponent },
  { path: 'games', component: GamesComponent },
  { path: 'games/:id/live', component: LiveGameComponent },
];
