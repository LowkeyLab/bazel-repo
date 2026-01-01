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

@Component({
  selector: 'app-create-circle',
  imports: [FormsModule],
  template: `
    <div class="container mx-auto px-4 py-8 max-w-2xl">
      <div class="mb-6">
        <button class="btn btn-ghost btn-sm" (click)="goBack()">← Back</button>
      </div>

      <div class="card bg-base-100 shadow-xl">
        <div class="card-body">
          <h1 class="card-title text-3xl mb-6">Create New Circle</h1>

          <form (submit)="onSubmit($event)">
            <div class="form-control w-full mb-4">
              <label class="label">
                <span class="label-text">Circle Name</span>
              </label>
              <input
                type="text"
                placeholder="Sunday Football Crew"
                class="input input-bordered w-full"
                [(ngModel)]="circleName"
                name="circleName"
                required
              />
              <label class="label">
                <span class="label-text-alt"
                  >Choose a fun name for your circle</span
                >
              </label>
            </div>

            <div class="alert alert-info mb-4">
              <span>
                Circles are created as the logged-in user
                {{ auth.currentUser()?.username || '' }}.
              </span>
            </div>

            @if (error()) {
              <div class="alert alert-error mb-4">
                <svg
                  xmlns="http://www.w3.org/2000/svg"
                  class="stroke-current shrink-0 h-6 w-6"
                  fill="none"
                  viewBox="0 0 24 24"
                >
                  <path
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    stroke-width="2"
                    d="M10 14l2-2m0 0l2-2m-2 2l-2-2m2 2l2 2m7-2a9 9 0 11-18 0 9 9 0 0118 0z"
                  />
                </svg>
                <span>{{ error() }}</span>
              </div>
            }

            <div class="card-actions justify-end">
              <button type="button" class="btn btn-ghost" (click)="goBack()">
                Cancel
              </button>
              <button
                type="submit"
                class="btn btn-primary"
                [disabled]="loading()"
              >
                @if (loading()) {
                  <span class="loading loading-spinner"></span>
                }
                Create Circle
              </button>
            </div>
          </form>
        </div>
      </div>
    </div>
  `,
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

  protected goBack(): void {
    this.router.navigate(['/circles']);
  }
}
