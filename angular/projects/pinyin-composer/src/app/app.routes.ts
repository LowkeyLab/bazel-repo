import { Routes } from '@angular/router';

export const routes: Routes = [
  {
    path: '',
    loadComponent: () =>
      import('./editor/pinyin-composer-page.component').then(
        (module) => module.PinyinComposerPageComponent,
      ),
  },
];
