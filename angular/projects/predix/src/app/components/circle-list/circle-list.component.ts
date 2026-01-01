import {
  ChangeDetectionStrategy,
  Component,
  signal,
  inject,
} from '@angular/core';
import { Router } from '@angular/router';
import { CircleService } from '../../services/circle.service';
import type { Circle } from '../../models/circle.model';

@Component({
  selector: 'app-circle-list',
  imports: [],
  template: `
    <div class="container mx-auto px-4 py-8">
      <div class="flex justify-between items-center mb-6">
        <h1 class="text-3xl font-bold">My Circles</h1>
        <button class="btn btn-primary" (click)="createCircle()">
          Create Circle
        </button>
      </div>

      @if (loading()) {
        <div class="flex justify-center">
          <span class="loading loading-spinner loading-lg"></span>
        </div>
      } @else if (circles().length === 0) {
        <div class="alert alert-info">
          <svg
            xmlns="http://www.w3.org/2000/svg"
            fill="none"
            viewBox="0 0 24 24"
            class="stroke-current shrink-0 w-6 h-6"
          >
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="2"
              d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
            ></path>
          </svg>
          <span>No circles yet. Create your first circle to get started!</span>
        </div>
      } @else {
        <div class="grid gap-4 md:grid-cols-2 lg:grid-cols-3">
          @for (circle of circles(); track circle.id) {
            <div
              class="card bg-base-100 shadow-xl hover:shadow-2xl transition-shadow cursor-pointer"
              (click)="viewCircle(circle.id)"
            >
              <div class="card-body">
                <h2 class="card-title">{{ circle.name }}</h2>
                <p class="text-sm text-secondary">
                  {{ circle.members.length }} members
                </p>
                <div class="card-actions justify-end">
                  <div class="badge badge-outline">
                    {{ getTotalClout(circle) }} total clout
                  </div>
                </div>
              </div>
            </div>
          }
        </div>
      }
    </div>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class CircleListComponent {
  private readonly router = inject(Router);
  private readonly circleService = inject(CircleService);

  protected readonly circles = signal<Circle[]>([]);
  protected readonly loading = signal(false);

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
