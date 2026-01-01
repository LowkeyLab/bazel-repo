import {
  ChangeDetectionStrategy,
  Component,
  OnInit,
  inject,
  signal,
} from '@angular/core';
import { FormsModule } from '@angular/forms';
import { ActivatedRoute, Router, RouterLink } from '@angular/router';

import { AuthService } from '../../services/auth.service';

@Component({
  selector: 'app-login',
  imports: [FormsModule, RouterLink],
  template: `
    <div class="min-h-screen flex items-center justify-center bg-base-200 px-4">
      <div class="card w-full max-w-md bg-base-100 shadow-xl">
        <div class="card-body space-y-4">
          <div class="flex items-center gap-2">
            <span class="text-2xl">🏟️</span>
            <div>
              <p class="text-sm text-gray-500">Welcome to</p>
              <h1 class="text-3xl font-bold">Predix</h1>
            </div>
          </div>

          <p class="text-sm text-gray-500">
            Sign in to create circles, open contests, and make predictions.
          </p>

          <form (submit)="onSubmit($event)" class="space-y-3">
            <div class="form-control">
              <label class="label">
                <span class="label-text">Username</span>
              </label>
              <input
                type="text"
                class="input input-bordered"
                [(ngModel)]="username"
                name="username"
                autocomplete="username"
                required
              />
            </div>

            <div class="form-control">
              <label class="label">
                <span class="label-text">Password</span>
              </label>
              <input
                type="password"
                class="input input-bordered"
                [(ngModel)]="password"
                name="password"
                autocomplete="current-password"
                required
              />
            </div>

            @if (error()) {
              <div class="alert alert-error">
                <span>{{ error() }}</span>
              </div>
            }

            <div class="card-actions justify-end">
              <button
                type="submit"
                class="btn btn-primary"
                [disabled]="loading()"
              >
                @if (loading()) {
                  <span class="loading loading-spinner"></span>
                }
                Sign in
              </button>
            </div>
          </form>

          <p class="text-xs text-gray-500">
            Tip: use any username/password to create a new account on first
            login.
          </p>
        </div>
      </div>
    </div>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class LoginComponent implements OnInit {
  private readonly auth = inject(AuthService);
  private readonly router = inject(Router);
  private readonly route = inject(ActivatedRoute);

  protected username = '';
  protected password = '';
  protected readonly loading = signal(false);
  protected readonly error = signal('');

  ngOnInit(): void {
    if (this.auth.isAuthenticated()) {
      this.redirect();
    }
  }

  protected onSubmit(event: Event): void {
    event.preventDefault();

    if (!this.username.trim() || !this.password.trim()) {
      this.error.set('Username and password are required');
      return;
    }

    this.loading.set(true);
    this.error.set('');

    this.auth.login(this.username.trim(), this.password).subscribe({
      next: () => {
        this.loading.set(false);
        this.redirect();
      },
      error: (err) => {
        this.loading.set(false);
        this.error.set(err.error?.error || 'Login failed');
      },
    });
  }

  private redirect(): void {
    const redirectTo =
      this.route.snapshot.queryParamMap.get('redirectTo') || '/circles';
    this.router.navigateByUrl(redirectTo);
  }
}
