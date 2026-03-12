import { TestBed } from '@angular/core/testing';
import { Router } from '@angular/router';
import { provideRouter } from '@angular/router';
import { signal } from '@angular/core';
import { LandingComponent } from './landing.component';
import { AuthService } from '../auth/auth.service';

describe('LandingComponent', () => {
  const mockAuthService = {
    isAuthenticated: signal(false),
    login: vi.fn(),
    logout: () => {},
    getToken: () => null,
    handleCallback: async () => 'fake-token',
  };

  beforeEach(async () => {
    await TestBed.configureTestingModule({
      imports: [LandingComponent],
      providers: [
        provideRouter([]),
        { provide: AuthService, useValue: mockAuthService },
      ],
    }).compileComponents();
    mockAuthService.login.mockReset();
  });

  it('should create', () => {
    mockAuthService.isAuthenticated = signal(false);
    const fixture = TestBed.createComponent(LandingComponent);
    expect(fixture.componentInstance).toBeTruthy();
  });

  it('should display sign-in button', () => {
    mockAuthService.isAuthenticated = signal(false);
    const fixture = TestBed.createComponent(LandingComponent);
    fixture.detectChanges();

    const button = fixture.nativeElement.querySelector(
      '[data-testid="sign-in-button"]',
    );
    expect(button).toBeTruthy();
    expect(button.textContent).toContain('Get Started');
  });

  it('should call login on sign-in click', () => {
    mockAuthService.isAuthenticated = signal(false);
    const fixture = TestBed.createComponent(LandingComponent);
    fixture.detectChanges();

    const button = fixture.nativeElement.querySelector(
      '[data-testid="sign-in-button"]',
    );
    button.click();

    expect(mockAuthService.login).toHaveBeenCalled();
  });

  it('should redirect to dashboard when authenticated', () => {
    const router = TestBed.inject(Router);
    const navigateSpy = vi.spyOn(router, 'navigate');

    mockAuthService.isAuthenticated = signal(true);
    const fixture = TestBed.createComponent(LandingComponent);
    fixture.detectChanges();

    expect(navigateSpy).toHaveBeenCalledWith(['/dashboard']);
  });
});
