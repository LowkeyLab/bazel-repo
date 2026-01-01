import { HttpInterceptorFn } from '@angular/common/http';
import { inject } from '@angular/core';

import { environment } from '../../environments/environment';
import { AuthService } from './auth.service';

export const authInterceptor: HttpInterceptorFn = (req, next) => {
  const auth = inject(AuthService);
  const token = auth.token();
  const isApiRequest = req.url.startsWith(environment.apiUrl);
  const isLogin = req.url.startsWith(`${environment.apiUrl}/login`);
  const isRegister = req.url.startsWith(`${environment.apiUrl}/register`);

  if (token && isApiRequest && !isLogin && !isRegister) {
    req = req.clone({
      setHeaders: {
        Authorization: `Bearer ${token}`,
      },
    });
  }

  return next(req);
};
