import {
  ChangeDetectionStrategy,
  Component,
  effect,
  inject,
} from '@angular/core';
import { Router } from '@angular/router';
import { AuthService } from '../auth/auth.service';

@Component({
  selector: 'app-landing',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="hero min-h-screen bg-base-200">
      <div class="hero-content text-center">
        <div class="max-w-lg">
          <h1 class="text-6xl font-extrabold tracking-tight mb-2">
            Nicknamer2
          </h1>
          <p class="text-lg text-base-content/60 mb-6">
            Discord Nickname Management
          </p>
          <p class="text-base-content/80 mb-8">
            Effortlessly manage nicknames across your Discord servers. View,
            organize, and bulk-update server member names from a single
            dashboard.
          </p>
          <button
            class="btn btn-primary btn-lg"
            data-testid="sign-in-button"
            (click)="onSignIn()"
          >
            Get Started
          </button>
        </div>
      </div>
    </div>
  `,
})
export class LandingComponent {
  private readonly authService = inject(AuthService);
  private readonly router = inject(Router);

  constructor() {
    effect(() => {
      if (this.authService.isAuthenticated()) {
        this.router.navigate(['/dashboard']);
      }
    });
  }

  protected onSignIn(): void {
    this.authService.login();
  }
}
