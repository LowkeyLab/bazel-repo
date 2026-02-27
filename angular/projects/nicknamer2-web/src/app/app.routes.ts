import { Routes } from '@angular/router';

export const routes: Routes = [
  { path: '', redirectTo: 'servers', pathMatch: 'full' },
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
