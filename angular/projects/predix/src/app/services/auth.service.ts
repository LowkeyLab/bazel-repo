import { Injectable, inject, signal } from '@angular/core';
import { HttpClient } from '@angular/common/http';
import { tap, type Observable } from 'rxjs';

import { environment } from '../../environments/environment';

export interface AuthUser {
  id: number;
  username: string;
  role: string;
}

export interface LoginResponse {
  token: string;
  user: AuthUser;
}

@Injectable({ providedIn: 'root' })
export class AuthService {
  private readonly http = inject(HttpClient);
  private readonly storageKey = 'predix_auth';
  private readonly currentUserSignal = signal<AuthUser | null>(null);
  private readonly tokenSignal = signal<string | null>(null);

  readonly currentUser = this.currentUserSignal.asReadonly();
  readonly token = this.tokenSignal.asReadonly();

  constructor() {
    this.restoreSession();
  }

  login(username: string, password: string): Observable<LoginResponse> {
    return this.http
      .post<LoginResponse>(`${environment.apiUrl}/login`, {
        username,
        password,
      })
      .pipe(tap((res) => this.persistSession(res)));
  }

  register(username: string, password: string): Observable<LoginResponse> {
    return this.http
      .post<LoginResponse>(`${environment.apiUrl}/register`, {
        username,
        password,
      })
      .pipe(tap((res) => this.persistSession(res)));
  }

  logout(): void {
    this.currentUserSignal.set(null);
    this.tokenSignal.set(null);
    localStorage.removeItem(this.storageKey);
  }

  isAuthenticated(): boolean {
    return !!this.tokenSignal();
  }

  private restoreSession(): void {
    const raw = localStorage.getItem(this.storageKey);
    if (!raw) return;

    try {
      const parsed = JSON.parse(raw) as LoginResponse;
      this.persistSession(parsed);
    } catch (err) {
      console.error('Failed to restore session', err);
      localStorage.removeItem(this.storageKey);
    }
  }

  private persistSession(res: LoginResponse): void {
    this.currentUserSignal.set(res.user);
    this.tokenSignal.set(res.token);
    localStorage.setItem(this.storageKey, JSON.stringify(res));
  }
}
