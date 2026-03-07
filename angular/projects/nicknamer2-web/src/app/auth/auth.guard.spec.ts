import { TestBed } from '@angular/core/testing';
import { signal } from '@angular/core';
import { authGuard } from './auth.guard';
import { AuthService } from './auth.service';

describe('authGuard', () => {
  const mockAuthService = {
    isAuthenticated: signal(true),
    login: vi.fn(),
    logout: () => {},
    getToken: () => 'fake-token',
    handleCallback: async () => 'fake-token',
  };

  beforeEach(() => {
    TestBed.configureTestingModule({
      providers: [{ provide: AuthService, useValue: mockAuthService }],
    });
    mockAuthService.login.mockReset();
  });

  it('should allow activation when authenticated', () => {
    mockAuthService.isAuthenticated = signal(true);

    const result = TestBed.runInInjectionContext(() =>
      authGuard(),
    );

    expect(result).toBe(true);
  });

  it('should block activation and redirect to login when not authenticated', () => {
    mockAuthService.isAuthenticated = signal(false);

    const result = TestBed.runInInjectionContext(() =>
      authGuard(),
    );

    expect(result).toBe(false);
    expect(mockAuthService.login).toHaveBeenCalled();
  });

  it('should not call login when authenticated', () => {
    mockAuthService.isAuthenticated = signal(true);

    TestBed.runInInjectionContext(() => authGuard());

    expect(mockAuthService.login).not.toHaveBeenCalled();
  });
});
