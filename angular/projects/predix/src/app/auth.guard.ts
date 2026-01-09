import { inject } from '@angular/core';
import { type CanActivateFn, Router } from '@angular/router';

import { AuthService } from './services/auth.service';

export const authGuard: CanActivateFn = (_route, state) => {
  const auth = inject(AuthService);
  const router = inject(Router);

  if (auth.isAuthenticated()) {
    return true;
  }

  // Redirect to login page which will trigger Authorizer flow
  // Pass the current URL so we can return after login
  const redirectTo = state.url;
  router.navigate(['/login'], {
    queryParams: { redirectTo },
  });

  return false;
};
