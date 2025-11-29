import { Routes } from '@angular/router';
import { LandingComponent } from './landing/landing.component';
import { GamesComponent } from './games/games.component';

export const routes: Routes = [
  { path: '', component: LandingComponent },
  { path: 'games', component: GamesComponent },
];
