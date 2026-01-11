import { Injectable, inject, signal } from '@angular/core';
import { from, map, switchMap } from 'rxjs';
import {
  ApiResponse,
  Authorizer,
  AuthorizeResponse,
  GetTokenResponse,
  ResponseTypes,
  User,
} from '@authorizerdev/authorizer-js';

import { environment } from '../../environments/environment';
import { UserId } from '../models/user.model';
import { LocalStorageService } from '../services/local-storage.service';

export interface AuthUser {
  id: UserId;
  username: string;
  role: string;
}

export interface LoginResponse {
  access_token: string;
  refresh_token: string;
  user: AuthUser;
}

interface TokenLoginResponse {
  access_token: string;
  refresh_token: string;
}

@Injectable({ providedIn: 'root' })
export class AuthService {
  private readonly authorizer = new Authorizer({
    authorizerURL: environment.authorizer.authorizerURL,
    redirectURL: window.location.origin + '/login',
    clientID: environment.authorizer.clientID,
  });

  private readonly localStorageService = inject(LocalStorageService);
  private readonly currentUserSignal = signal<AuthUser | null>(null);
  private readonly tokenSignal = signal<string | null>(null);

  readonly currentUser = this.currentUserSignal.asReadonly();
  readonly token = this.tokenSignal.asReadonly();

  login(): void {
    if (this.isAuthenticated()) {
      return;
    }
    from(
      this.authorizer.authorize({
        response_type: ResponseTypes.Token,
        use_refresh_token: true,
      }),
    )
      .pipe(
        map((res: ApiResponse<GetTokenResponse | AuthorizeResponse>) => {
          if (res.errors || !res.data) {
            throw new Error('Login failed');
          }

          let data = res.data;

          if ('access_token' in data) {
            return {
              access_token: data.access_token,
              refresh_token: data.refresh_token!,
            } as TokenLoginResponse;
          } else {
            throw new Error('Invalid token response');
          }
        }),
        switchMap(({ access_token, refresh_token }: TokenLoginResponse) =>
          from(
            this.authorizer.getProfile({
              Authorization: `Bearer ${access_token}`,
            }),
          ).pipe(
            map((profileRes: ApiResponse<User>) => {
              if (profileRes.errors || !profileRes.data) {
                throw new Error('Failed to fetch user profile');
              }

              const user: AuthUser = this.mapUser(profileRes.data);

              return {
                access_token,
                refresh_token,
                user,
              } as LoginResponse;
            }),
          ),
        ),
      )
      .subscribe({
        next: (res: LoginResponse) => {
          this.persistSession(res);
        },
        error: () => {
          this.logout();
        },
      });
  }

  logout(): void {
    this.authorizer.logout();
    this.currentUserSignal.set(null);
    this.tokenSignal.set(null);
    this.localStorageService.removeAuthData();
    window.location.href = '/';
  }

  isAuthenticated(): boolean {
    return !!this.currentUserSignal();
  }

  private persistSession(res: LoginResponse): void {
    this.currentUserSignal.set(res.user);
    this.tokenSignal.set(res.access_token);
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
