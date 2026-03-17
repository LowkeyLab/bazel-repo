import { Routes } from '@angular/router';
import { authGuard } from './auth/auth.guard';

export const routes: Routes = [
  {
    path: '',
    loadComponent: () =>
      import('./landing/landing.component').then((m) => m.LandingComponent),
  },
  {
    path: 'dashboard',
    loadComponent: () =>
      import('./dashboard/dashboard.component').then(
        (m) => m.DashboardComponent,
      ),
    canActivate: [authGuard],
  },
  {
    path: 'servers/new',
    loadComponent: () =>
      import('./servers/add-server.component').then(
        (m) => m.AddServerComponent,
      ),
    canActivate: [authGuard],
  },
  {
    path: 'servers',
    loadComponent: () =>
      import('./servers/server-list.component').then(
        (m) => m.ServerListComponent,
      ),
    canActivate: [authGuard],
  },
  {
    path: 'servers/:serverId/names/batch',
    loadComponent: () =>
      import('./servers/batch-add-names.component').then(
        (m) => m.BatchAddNamesComponent,
      ),
    canActivate: [authGuard],
  },
  {
    path: 'servers/:serverId/names',
    loadComponent: () =>
      import('./servers/server-names.component').then(
        (m) => m.ServerNamesComponent,
      ),
    canActivate: [authGuard],
  },
  {
    path: 'callback',
    loadComponent: () =>
      import('./auth/callback.component').then((m) => m.CallbackComponent),
  },
];
