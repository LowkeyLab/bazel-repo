import { ChangeDetectionStrategy, Component, inject } from '@angular/core';
import { RouterLink, RouterOutlet } from '@angular/router';
import { AuthService } from './auth/auth.service';

@Component({
  selector: 'app-root',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [RouterOutlet, RouterLink],
  template: `
    <div class="navbar bg-base-200 px-4">
      <div class="flex-1">
        <a
          [routerLink]="authService.isAuthenticated() ? '/dashboard' : '/'"
          class="text-xl font-bold"
          >nicknamer2</a
        >
      </div>
      <div class="flex-none">
        @if (authService.isAuthenticated()) {
          <button
            class="btn btn-ghost btn-sm"
            data-testid="logout-button"
            (click)="authService.logout()"
          >
            Logout
          </button>
        } @else {
          <button
            class="btn btn-primary btn-sm"
            data-testid="login-button"
            (click)="authService.login()"
          >
            Login
          </button>
        }
      </div>
    </div>
    <router-outlet />
  `,
})
export class App {
  protected readonly authService = inject(AuthService);
}
