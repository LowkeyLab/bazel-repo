import {
  ChangeDetectionStrategy,
  Component,
  signal,
  inject,
  OnInit,
} from '@angular/core';
import { ActivatedRoute, Router } from '@angular/router';
import { CircleService } from '../../services/circle.service';
import type { Circle } from '../../models/circle.model';
import { DatePipe } from '@angular/common';

@Component({
  selector: 'app-circle-detail',
  imports: [DatePipe],
  template: `
    <div class="container mx-auto px-4 py-8">
      @if (loading()) {
        <div class="flex justify-center">
          <span class="loading loading-spinner loading-lg"></span>
        </div>
      } @else if (circle(); as circle) {
        <div class="mb-6">
          <button class="btn btn-ghost btn-sm" (click)="goBack()">
            ← Back
          </button>
        </div>

        <div class="card bg-base-100 shadow-xl">
          <div class="card-body">
            <h1 class="card-title text-4xl mb-4">{{ circle.name }}</h1>
            <p class="text-sm text-secondary">
              Created {{ circle.created_at | date: 'medium' }}
            </p>

            <div class="divider"></div>

            <h2 class="text-2xl font-bold mb-4">Members</h2>
            <div class="overflow-x-auto">
              <table class="table">
                <thead>
                  <tr>
                    <th>User ID</th>
                    <th>💎 Clout</th>
                    <th></th>
                  </tr>
                </thead>
                <tbody>
                  @for (member of circle.members; track member.user_id) {
                    <tr>
                      <td>{{ member.user_id }}</td>
                      <td>
                        <div class="badge badge-primary">
                          {{ member.clout }}
                        </div>
                      </td>
                      <td>
                        @if (member.clout === getMaxClout(circle)) {
                          <div class="badge badge-warning">👑 Top Dog</div>
                        }
                      </td>
                    </tr>
                  }
                </tbody>
              </table>
            </div>

            <div class="card-actions justify-end mt-4">
              <button class="btn btn-primary" (click)="addMember()">
                Add Member
              </button>
            </div>
          </div>
        </div>

        <div class="mt-8">
          <h2 class="text-2xl font-bold mb-4">Contests</h2>
          <div class="alert alert-info">
            <span>Contest list coming soon...</span>
          </div>
        </div>
      } @else {
        <div class="alert alert-error">
          <span>Circle not found</span>
        </div>
      }
    </div>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class CircleDetailComponent implements OnInit {
  private readonly route = inject(ActivatedRoute);
  private readonly router = inject(Router);
  private readonly circleService = inject(CircleService);

  protected readonly circle = signal<Circle | null>(null);
  protected readonly loading = signal(true);

  ngOnInit(): void {
    const id = Number(this.route.snapshot.paramMap.get('id'));
    if (id) {
      this.loadCircle(id);
    }
  }

  private loadCircle(id: number): void {
    this.circleService.getCircle(id).subscribe({
      next: (circle) => {
        this.circle.set(circle);
        this.loading.set(false);
      },
      error: () => {
        this.loading.set(false);
      },
    });
  }

  protected goBack(): void {
    this.router.navigate(['/circles']);
  }

  protected addMember(): void {
    // TODO: Implement add member modal/form
    alert('Add member functionality coming soon!');
  }

  protected getMaxClout(circle: Circle): number {
    return Math.max(...circle.members.map((m) => m.clout));
  }
}
