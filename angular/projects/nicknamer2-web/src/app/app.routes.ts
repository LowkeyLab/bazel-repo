import { Routes } from '@angular/router';

export const routes: Routes = [
  {
    path: '',
    loadComponent: () =>
      import('./dashboard/dashboard.component').then(
        (m) => m.DashboardComponent,
      ),
  },
  {
    path: 'servers',
    loadComponent: () =>
      import('./servers/server-list.component').then(
        (m) => m.ServerListComponent,
      ),
  },
  {
    path: 'servers/:serverId/names',
    loadComponent: () =>
      import('./servers/server-names.component').then(
        (m) => m.ServerNamesComponent,
      ),
  },
];
