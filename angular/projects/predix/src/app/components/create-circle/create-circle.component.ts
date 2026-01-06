import {
  ChangeDetectionStrategy,
  Component,
  signal,
  inject,
} from '@angular/core';
import { Router } from '@angular/router';
import { FormsModule } from '@angular/forms';
import { CircleService } from '../../services/circle.service';
import { AuthService } from '../../services/auth.service';
import { BackButtonComponent } from '../back-button/back-button.component';

@Component({
  selector: 'create-circle',
  imports: [FormsModule, BackButtonComponent],
  templateUrl: './create-circle.component.html',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class CreateCircleComponent {
  private readonly router = inject(Router);
  private readonly circleService = inject(CircleService);
  protected readonly auth = inject(AuthService);

  protected circleName = '';
  protected readonly loading = signal(false);
  protected readonly error = signal('');

  protected onSubmit(event: Event): void {
    event.preventDefault();

    if (!this.circleName.trim()) {
      this.error.set('Circle name is required');
      return;
    }

    this.loading.set(true);
    this.error.set('');

    this.circleService
      .createCircle({
        name: this.circleName,
      })
      .subscribe({
        next: (circle) => {
          this.loading.set(false);
          this.router.navigate(['/circles', circle.id]);
        },
        error: (err) => {
          this.loading.set(false);
          this.error.set(err.error?.error || 'Failed to create circle');
        },
      });
  }

  protected navigateBack(): void {
    this.router.navigate(['/circles']);
  }
}
