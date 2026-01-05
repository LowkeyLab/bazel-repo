import {
  ChangeDetectionStrategy,
  Component,
  signal,
  inject,
} from '@angular/core';
import { Router } from '@angular/router';
import { ContestService } from '../../services/contest.service';
import type { Contest, ContestStatus } from '../../models/contest.model';
import { DatePipe } from '@angular/common';

@Component({
  selector: 'contest-list',
  imports: [DatePipe],
  template: `
    <div class="container mx-auto px-4 py-8">
      <h1 class="text-3xl font-bold mb-6">Contests in Your Circles</h1>

      @if (loading()) {
        <div class="flex justify-center">
          <span class="loading loading-spinner loading-lg"></span>
        </div>
      } @else if (contests().length === 0) {
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
          <span>
            No contests in your circles yet. Go to a circle to create a contest!
          </span>
        </div>
      } @else {
        <div class="grid gap-4">
          @for (contest of contests(); track contest.id) {
            <div
              class="card bg-base-100 shadow-xl hover:shadow-2xl transition-shadow cursor-pointer"
              (click)="viewContest(contest.id)"
            >
              <div class="card-body">
                <div class="flex justify-between items-start">
                  <div class="flex-1">
                    <h2 class="card-title text-2xl mb-2">
                      {{ contest.question }}
                    </h2>
                    <p class="text-sm text-secondary mb-3">
                      Predictions close {{ contest.expires_at | date: 'short' }}
                    </p>
                    <div class="flex gap-2 flex-wrap mb-2">
                      @for (option of contest.options; track option.id) {
                        <div class="badge badge-outline">{{ option.text }}</div>
                      }
                    </div>
                  </div>
                  <div class="flex flex-col items-end gap-2">
                    <div
                      class="badge"
                      [class.badge-success]="contest.status === 'OPEN'"
                      [class.badge-neutral]="contest.status === 'CLOSED'"
                      [class.badge-info]="contest.status === 'RESOLVED'"
                    >
                      {{ statusLabels[contest.status] }}
                    </div>
                  </div>
                </div>
                <div class="card-actions justify-between items-center">
                  <span class="text-sm text-secondary"
                    >{{ contest.predictions.length }} predictions</span
                  >
                  <span class="text-sm text-secondary"
                    >Min stake: {{ contest.min_stake }} Clout</span
                  >
                  <span class="text-sm font-bold"
                    >💎 {{ getTotalClout(contest) }} Clout staked</span
                  >
                </div>
                @if (contest.total_pot > 0) {
                  <div class="divider my-2"></div>
                  <div class="grid grid-cols-3 gap-4 text-sm">
                    <div>
                      <p class="text-secondary">Total Pot</p>
                      <p class="font-bold">💎 {{ contest.total_pot }}</p>
                    </div>
                    <div>
                      <p class="text-secondary">Consumed (10%)</p>
                      <p class="font-bold text-warning">
                        💎 {{ contest.clout_consumed }}
                      </p>
                    </div>
                    <div>
                      <p class="text-secondary">Remaining (90%)</p>
                      <p class="font-bold text-success">
                        💎 {{ contest.total_pot - contest.clout_consumed }}
                      </p>
                    </div>
                  </div>
                }
              </div>
            </div>
          }
        </div>
      }
    </div>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ContestListComponent {
  private readonly router = inject(Router);
  private readonly contestService = inject(ContestService);

  protected readonly contests = signal<Contest[]>([]);
  protected readonly loading = signal(false);
  protected readonly statusLabels: Record<ContestStatus, string> = {
    OPEN: 'Open',
    CLOSED: 'Closed (paused)',
    RESOLVED: 'Resolved',
  };

  viewContest(id: number): void {
    this.router.navigate(['/contests', id]);
  }

  protected getTotalClout(contest: Contest): number {
    return contest.predictions.reduce((sum, pred) => sum + pred.clout, 0);
  }
}
