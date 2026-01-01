import { Routes } from '@angular/router';
import { authGuard } from './auth.guard';

export const routes: Routes = [
  {
    path: '',
    loadComponent: () =>
      import('./components/home/home.component').then((m) => m.HomeComponent),
  },
  {
    path: 'login',
    loadComponent: () =>
      import('./components/login/login.component').then(
        (m) => m.LoginComponent,
      ),
  },
  {
    path: 'circles',
    canActivate: [authGuard],
    loadComponent: () =>
      import('./components/circle-list/circle-list.component').then(
        (m) => m.CircleListComponent,
      ),
  },
  {
    path: 'circles/new',
    canActivate: [authGuard],
    loadComponent: () =>
      import('./components/create-circle/create-circle.component').then(
        (m) => m.CreateCircleComponent,
      ),
  },
  {
    path: 'circles/:id',
    canActivate: [authGuard],
    loadComponent: () =>
      import('./components/circle-detail/circle-detail.component').then(
        (m) => m.CircleDetailComponent,
      ),
  },
  {
    path: 'contests',
    canActivate: [authGuard],
    loadComponent: () =>
      import('./components/contest-list/contest-list.component').then(
        (m) => m.ContestListComponent,
      ),
  },
  {
    path: 'contests/new',
    canActivate: [authGuard],
    loadComponent: () =>
      import('./components/create-contest/create-contest.component').then(
        (m) => m.CreateContestComponent,
      ),
  },
  {
    path: 'contests/:id',
    canActivate: [authGuard],
    loadComponent: () =>
      import('./components/contest-detail/contest-detail.component').then(
        (m) => m.ContestDetailComponent,
      ),
  },
  {
    path: '**',
    redirectTo: '/',
  },
];
