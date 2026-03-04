import { Injectable, signal, computed } from '@angular/core';
import { casdoorSdk } from './auth.config';

const TOKEN_KEY = 'casdoor_access_token';

@Injectable({ providedIn: 'root' })
export class AuthService {
  private readonly tokenSignal = signal<string | null>(
    localStorage.getItem(TOKEN_KEY),
  );

  readonly isAuthenticated = computed(() => this.tokenSignal() !== null);

  getToken(): string | null {
    return this.tokenSignal();
  }

  login(): void {
    const url = casdoorSdk.getSigninUrl();
    window.location.href = url;
  }

  async handleCallback(): Promise<string> {
    const urlParams = new URLSearchParams(window.location.search);
    const code = urlParams.get('code');

    if (!code) {
      throw new Error('No authorization code in callback URL');
    }

    const response = await casdoorSdk.exchangeForAccessToken();
    const accessToken = response.access_token;

    localStorage.setItem(TOKEN_KEY, accessToken);
    this.tokenSignal.set(accessToken);
    return accessToken;
  }

  logout(): void {
    localStorage.removeItem(TOKEN_KEY);
    this.tokenSignal.set(null);
  }
}
