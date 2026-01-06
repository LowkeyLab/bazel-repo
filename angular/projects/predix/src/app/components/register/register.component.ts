import {
  ChangeDetectionStrategy,
  Component,
  OnInit,
  inject,
  signal,
} from '@angular/core';
import { FormsModule } from '@angular/forms';
import { ActivatedRoute, Router, RouterLink } from '@angular/router';
import { finalize } from 'rxjs/operators';

import { AuthService } from '../../services/auth.service';

@Component({
  selector: 'register',
  imports: [FormsModule, RouterLink],
  templateUrl: './register.component.html',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class RegisterComponent implements OnInit {
  private readonly auth = inject(AuthService);
  private readonly router = inject(Router);
  private readonly route = inject(ActivatedRoute);

  protected username = '';
  protected password = '';
  protected confirm = '';
  protected readonly loading = signal(false);
  protected readonly error = signal('');

  ngOnInit(): void {
    if (this.auth.isAuthenticated()) {
      this.redirect();
    }
  }

  protected onSubmit(event: Event): void {
    event.preventDefault();

    const username = this.username.trim();
    const password = this.password.trim();
    const confirm = this.confirm.trim();

    if (!username || !password || !confirm) {
      this.error.set('All fields are required');
      return;
    }

    if (password.length < 6) {
      this.error.set('Password must be at least 6 characters');
      return;
    }

    if (password !== confirm) {
      this.error.set('Passwords must match');
      return;
    }

    this.loading.set(true);
    this.error.set('');

    this.auth
      .register(username, password)
      .pipe(finalize(() => this.loading.set(false)))
      .subscribe({
        next: () => this.redirect(),
        error: (err) => {
          const message = err?.error?.error ?? 'Registration failed';
          this.error.set(message);
        },
      });
  }

  private redirect(): void {
    const redirectTo =
      this.route.snapshot.queryParamMap.get('redirectTo') || '/circles';
    this.router.navigateByUrl(redirectTo);
  }
}
