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
  selector: 'join-circle',
  imports: [],
  templateUrl: './join-circle.component.html',
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
