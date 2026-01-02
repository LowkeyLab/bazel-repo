import {
  ChangeDetectionStrategy,
  Component,
  signal,
  inject,
  OnInit,
} from '@angular/core';
import { ActivatedRoute, Router } from '@angular/router';
import { CircleService } from '../../services/circle.service';

@Component({
  selector: 'app-join-circle',
  imports: [],
  template: `
    <div class="container mx-auto px-4 py-8">
      <div class="flex flex-col items-center justify-center min-h-[60vh]">
        @if (loading()) {
          <div class="flex flex-col items-center gap-4">
            <span class="loading loading-spinner loading-lg"></span>
            <p class="text-lg">Joining circle...</p>
          </div>
        } @else if (success()) {
          <div class="alert alert-success max-w-md">
            <svg
              xmlns="http://www.w3.org/2000/svg"
              class="h-6 w-6 shrink-0 stroke-current"
              fill="none"
              viewBox="0 0 24 24"
            >
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="2"
                d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z"
              />
            </svg>
            <div>
              <h3 class="font-bold">Success!</h3>
              <div class="text-sm">You've joined the circle</div>
            </div>
          </div>
          <button class="btn btn-primary mt-6" (click)="viewCircle()">
            View Circle
          </button>
        } @else if (error(); as errorMsg) {
          <div class="alert alert-error max-w-md">
            <svg
              xmlns="http://www.w3.org/2000/svg"
              class="h-6 w-6 shrink-0 stroke-current"
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
            <div>
              <h3 class="font-bold">Error</h3>
              <div class="text-sm">{{ errorMsg }}</div>
            </div>
          </div>
          <button class="btn btn-ghost mt-6" (click)="goToCircles()">
            View My Circles
          </button>
        }
      </div>
    </div>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class JoinCircleComponent implements OnInit {
  private readonly route = inject(ActivatedRoute);
  private readonly router = inject(Router);
  private readonly circleService = inject(CircleService);

  protected readonly loading = signal(true);
  protected readonly success = signal(false);
  protected readonly error = signal<string | null>(null);
  private circleId: number | null = null;

  ngOnInit(): void {
    const id = Number(this.route.snapshot.paramMap.get('id'));
    if (id) {
      this.circleId = id;
      this.joinCircle(id);
    } else {
      this.error.set('Invalid circle ID');
      this.loading.set(false);
    }
  }

  private joinCircle(id: number): void {
    this.circleService.joinCircle(id).subscribe({
      next: () => {
        this.loading.set(false);
        this.success.set(true);
      },
      error: (err) => {
        this.loading.set(false);
        const errorMessage =
          err.error?.error || 'Failed to join circle. Please try again.';
        this.error.set(errorMessage);
      },
    });
  }

  protected viewCircle(): void {
    if (this.circleId) {
      this.router.navigate(['/circles', this.circleId]);
    }
  }

  protected goToCircles(): void {
    this.router.navigate(['/circles']);
  }
}
