import { inject } from '@angular/core';
import { type CanActivateFn, Router } from '@angular/router';

import { AuthService } from './services/auth.service';

export const authGuard: CanActivateFn = (_route, state) => {
  const auth = inject(AuthService);
  const router = inject(Router);

  if (auth.isAuthenticated()) {
    return true;
  }

  const redirectTo =
    state.url && state.url !== '/login' ? state.url : '/circles';
  router.navigate(['/login'], {
    queryParams: { redirectTo },
  });

  return false;
};
