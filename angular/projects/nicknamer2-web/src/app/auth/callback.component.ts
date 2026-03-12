import {
  ChangeDetectionStrategy,
  Component,
  inject,
  OnInit,
  signal,
} from '@angular/core';
import { Router } from '@angular/router';
import { AuthService } from './auth.service';

@Component({
  selector: 'app-callback',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    @if (error()) {
      <div class="flex items-center justify-center min-h-screen">
        <div class="alert alert-error max-w-md">
          <span>{{ error() }}</span>
        </div>
      </div>
    } @else {
      <div class="flex items-center justify-center min-h-screen">
        <span class="loading loading-spinner loading-lg"></span>
        <span class="ml-2">Completing login...</span>
      </div>
    }
  `,
})
export class CallbackComponent implements OnInit {
  private readonly authService = inject(AuthService);
  private readonly router = inject(Router);

  protected readonly error = signal<string | null>(null);

  async ngOnInit(): Promise<void> {
    try {
      await this.authService.handleCallback();
      this.router.navigate(['/dashboard']);
    } catch (err) {
      this.error.set(err instanceof Error ? err.message : 'Login failed');
    }
  }
}
