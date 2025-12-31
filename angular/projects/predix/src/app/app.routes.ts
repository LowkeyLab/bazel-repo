import { Routes } from '@angular/router';

export const routes: Routes = [
  {
    path: '',
    redirectTo: '/circles',
    pathMatch: 'full',
  },
  {
    path: 'circles',
    loadComponent: () =>
      import('./components/circle-list/circle-list.component').then(
        (m) => m.CircleListComponent,
      ),
  },
  {
    path: 'circles/new',
    loadComponent: () =>
      import('./components/create-circle/create-circle.component').then(
        (m) => m.CreateCircleComponent,
      ),
  },
  {
    path: 'circles/:id',
    loadComponent: () =>
      import('./components/circle-detail/circle-detail.component').then(
        (m) => m.CircleDetailComponent,
      ),
  },
  {
    path: 'contests',
    loadComponent: () =>
      import('./components/contest-list/contest-list.component').then(
        (m) => m.ContestListComponent,
      ),
  },
  {
    path: 'contests/new',
    loadComponent: () =>
      import('./components/create-contest/create-contest.component').then(
        (m) => m.CreateContestComponent,
      ),
  },
  {
    path: 'contests/:id',
    loadComponent: () =>
      import('./components/contest-detail/contest-detail.component').then(
        (m) => m.ContestDetailComponent,
      ),
  },
];
