import { Injectable, inject, signal } from '@angular/core';
import { Router } from '@angular/router';
import { from } from 'rxjs';
import { Authorizer, ResponseTypes } from '@authorizerdev/authorizer-js';

import { environment } from '../../environments/environment';
import { UserId } from '../models/user.model';
import { LocalStorageService } from '../services/local-storage.service';

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

  private readonly localStorageService = inject(LocalStorageService);
  private readonly currentUserSignal = signal<AuthUser | null>(null);
  private readonly tokenSignal = signal<string | null>(null);

  readonly currentUser = this.currentUserSignal.asReadonly();
  readonly token = this.tokenSignal.asReadonly();

  login(): void {
    const loggedIn = this.tokenSignal();
    if (loggedIn) {
      return;
    }
    this.authorizer.authorize({
      response_type: ResponseTypes.Token,
    });
  }

  loginWithToken(token: string): void {
    this.fetchProfile(token);
  }

  logout(): void {
    this.authorizer.logout();
    this.currentUserSignal.set(null);
    this.tokenSignal.set(null);
    this.localStorageService.removeAuthData();
    window.location.href = '/';
  }

  isAuthenticated(): boolean {
    return !!this.tokenSignal();
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
    this.localStorageService.setAuthData(res);
  }

  private mapUser(user: any): AuthUser {
    return {
      id: user.id,
      username: user.email || user.id,
      role: user.roles,
    };
  }
}
