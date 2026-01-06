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
  selector: 'login',
  imports: [FormsModule, RouterLink],
  templateUrl: './login.component.html',
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

    const username = this.username.trim();
    const password = this.password.trim();

    if (!username || !password) {
      this.error.set('Username and password are required');
      return;
    }

    this.loading.set(true);
    this.error.set('');

    this.auth
      .login(username, password)
      .pipe(finalize(() => this.loading.set(false)))
      .subscribe({
        next: () => this.redirect(),
        error: (err) => {
          const message =
            err?.error?.error ||
            'Login failed. Please check your credentials and try again.';
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
