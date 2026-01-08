import {
  ChangeDetectionStrategy,
  Component,
  signal,
  inject,
  OnInit,
} from '@angular/core';
import { Router } from '@angular/router';
import { CircleService } from '../../services/circle.service';
import type { Circle } from '../../models/circle.model';

@Component({
  selector: 'circle-list',
  imports: [],
  templateUrl: './circle-list.component.html',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class CircleListComponent implements OnInit {
  private readonly router = inject(Router);
  private readonly circleService = inject(CircleService);

  public readonly circles = signal<Circle[]>([]);
  public readonly loading = signal(false);

  ngOnInit(): void {
    this.loadCircles();
  }

  private loadCircles(): void {
    this.loading.set(true);
    this.circleService.listUserCircles().subscribe({
      next: (circles) => {
        this.circles.set(circles);
        this.loading.set(false);
      },
      error: (err) => {
        console.error('Failed to load circles:', err);
        this.loading.set(false);
      },
    });
  }

  refreshCircles(): void {
    this.loadCircles();
  }

  createCircle(): void {
    this.router.navigate(['/circles/new']);
  }

  viewCircle(id: number): void {
    this.router.navigate(['/circles', id]);
  }

  protected getTotalClout(circle: Circle): number {
    return circle.members.reduce((sum, member) => sum + member.clout, 0);
  }
}
