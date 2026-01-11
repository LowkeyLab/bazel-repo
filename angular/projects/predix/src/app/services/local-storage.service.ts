import { Injectable } from '@angular/core';
import { AuthUser } from './auth.service';

export interface StoredAuthData {
  access_token: string;
  user: AuthUser;
}

@Injectable({ providedIn: 'root' })
export class LocalStorageService {
  // Centralized storage keys
  private readonly keys = {
    AUTH_DATA: 'predix_auth',
    POST_LOGIN_REDIRECT: 'post_login_redirect',
  } as const;

  /**
   * Get JWT token from local storage
   */
  getAccessToken(): string | null {
    const authData = this.getAuthData();
    return authData?.access_token ?? null;
  }

  /**
   * Get logged in user from local storage
   */
  getUser(): AuthUser | null {
    const authData = this.getAuthData();
    return authData?.user ?? null;
  }

  /**
   * Get full auth data (token + user) from local storage
   */
  getAuthData(): StoredAuthData | null {
    const raw = this.getItem(this.keys.AUTH_DATA);
    if (!raw) return null;

    try {
      return JSON.parse(raw) as StoredAuthData;
    } catch {
      // Invalid JSON, clear it
      this.removeAuthData();
      return null;
    }
  }

  /**
   * Store auth data (token + user) in local storage
   */
  setAuthData(data: StoredAuthData): void {
    this.setItem(this.keys.AUTH_DATA, JSON.stringify(data));
  }

  /**
   * Remove auth data from local storage
   */
  removeAuthData(): void {
    this.removeItem(this.keys.AUTH_DATA);
  }

  /**
   * Get post-login redirect URL
   */
  getPostLoginRedirect(): string | null {
    return this.getItem(this.keys.POST_LOGIN_REDIRECT);
  }

  /**
   * Set post-login redirect URL
   */
  setPostLoginRedirect(url: string): void {
    this.setItem(this.keys.POST_LOGIN_REDIRECT, url);
  }

  /**
   * Remove post-login redirect URL
   */
  removePostLoginRedirect(): void {
    this.removeItem(this.keys.POST_LOGIN_REDIRECT);
  }

  /**
   * Generic get item from local storage
   */
  getItem(key: string): string | null {
    try {
      return localStorage.getItem(key);
    } catch {
      return null;
    }
  }

  /**
   * Generic set item in local storage
   */
  setItem(key: string, value: string): void {
    try {
      localStorage.setItem(key, value);
    } catch (error) {
      console.error('Failed to save to localStorage:', error);
    }
  }

  /**
   * Generic remove item from local storage
   */
  removeItem(key: string): void {
    try {
      localStorage.removeItem(key);
    } catch (error) {
      console.error('Failed to remove from localStorage:', error);
    }
  }

  /**
   * Clear all items from local storage
   */
  clear(): void {
    try {
      localStorage.clear();
    } catch (error) {
      console.error('Failed to clear localStorage:', error);
    }
  }
}
