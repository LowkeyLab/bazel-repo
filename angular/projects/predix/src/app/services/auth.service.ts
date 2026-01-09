import { Injectable, inject, signal } from '@angular/core';
import { Router } from '@angular/router';
import { from } from 'rxjs';
import { Authorizer, ResponseTypes } from '@authorizerdev/authorizer-js';

import { environment } from '../../environments/environment';
import { UserId } from '../models/user.model';

export interface AuthUser {
  id: UserId;
  username: string;
  role: string;
}

export interface LoginResponse {
  token: string;
  user: AuthUser;
}

@Injectable({ providedIn: 'root' })
export class AuthService {
  private readonly router = inject(Router);
  private readonly authorizer = new Authorizer({
    authorizerURL: environment.authorizer.authorizerURL,
    redirectURL: environment.authorizer.redirectURL,
    clientID: environment.authorizer.clientID,
  });

  private readonly storageKey = 'predix_auth';
  private readonly redirectKey = 'post_login_redirect';
  private readonly currentUserSignal = signal<AuthUser | null>(null);
  private readonly tokenSignal = signal<string | null>(null);

  readonly currentUser = this.currentUserSignal.asReadonly();
  readonly token = this.tokenSignal.asReadonly();

  constructor() {
    this.restoreSession();
  }

  login(): void {
    this.authorizer.authorize({
      response_type: ResponseTypes.Token,
    });
  }

  register(): void {
    this.authorizer.authorize({
      response_type: ResponseTypes.Token,
    });
  }

  logout(): void {
    this.authorizer.logout();
    this.currentUserSignal.set(null);
    this.tokenSignal.set(null);
    localStorage.removeItem(this.storageKey);
    window.location.href = '/';
  }

  isAuthenticated(): boolean {
    return !!this.tokenSignal();
  }

  private restoreSession(): void {
    const searchParams = new URLSearchParams(window.location.search);
    const hashParams = new URLSearchParams(window.location.hash.substring(1));
    const accessToken =
      searchParams.get('access_token') || hashParams.get('access_token');

    if (accessToken) {
      this.fetchProfile(accessToken);

      // Clean URL
      window.history.replaceState({}, document.title, window.location.pathname);

      // Handle redirect
      const redirectTo = localStorage.getItem(this.redirectKey) || '/circles';
      localStorage.removeItem(this.redirectKey);
      this.router.navigateByUrl(redirectTo);
      return;
    }

    this.checkExistingSession();
  }

  private checkExistingSession(): void {
    const raw = localStorage.getItem(this.storageKey);
    if (raw) {
      try {
        const parsed = JSON.parse(raw) as LoginResponse;
        this.currentUserSignal.set(parsed.user);
        this.tokenSignal.set(parsed.token);
      } catch (err) {
        localStorage.removeItem(this.storageKey);
      }
    }

    from(this.authorizer.getSession()).subscribe({
      next: (res: any) => {
        if (res.user && res.access_token) {
          const mappedUser = this.mapUser(res.user);
          this.persistSession({ token: res.access_token, user: mappedUser });
        } else if (res.errors && res.errors.length > 0) {
          if (this.tokenSignal()) {
            this.logout();
          }
        }
      },
      error: () => {
        // keep local state if network error, or handle otherwise
      },
    });
  }

  private fetchProfile(accessToken: string): void {
    from(
      this.authorizer.getProfile({
        Authorization: `Bearer ${accessToken}`,
      }),
    ).subscribe({
      next: (res: any) => {
        if (res && !res.errors) {
          const user = this.mapUser(res);
          this.persistSession({ token: accessToken, user });
        }
      },
    });
  }

  private persistSession(res: LoginResponse): void {
    this.currentUserSignal.set(res.user);
    this.tokenSignal.set(res.token);
    localStorage.setItem(this.storageKey, JSON.stringify(res));
  }

  private mapUser(user: any): AuthUser {
    return {
      id: user.id,
      username: user.email || user.id,
      role: user.roles,
    };
  }
}
