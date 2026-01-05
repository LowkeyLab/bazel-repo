import {
  ChangeDetectionStrategy,
  Component,
  signal,
  inject,
  OnInit,
} from '@angular/core';
import { ActivatedRoute, Router } from '@angular/router';
import { ContestService } from '../../services/contest.service';
import type { Contest, ContestStatus } from '../../models/contest.model';
import { DatePipe } from '@angular/common';
import { FormsModule } from '@angular/forms';
import { AuthService } from '../../services/auth.service';
import { BackButtonComponent } from '../back-button/back-button.component';

@Component({
  selector: 'contest-detail',
  imports: [DatePipe, FormsModule, BackButtonComponent],
  template: `
    <div class="container mx-auto px-4 py-8">
      @if (loading()) {
        <div class="flex justify-center">
          <span class="loading loading-spinner loading-lg"></span>
        </div>
      } @else if (contest(); as contest) {
        <div class="mb-6">
          <back-button [link]="'/contests'" />
        </div>

        <div class="card bg-base-100 shadow-xl mb-6">
          <div class="card-body">
            <div class="flex justify-between items-start mb-4">
              <h1 class="text-3xl font-bold flex-1">{{ contest.question }}</h1>
              <div
                class="badge badge-lg"
                [class.badge-success]="contest.status === 'OPEN'"
                [class.badge-neutral]="contest.status === 'CLOSED'"
                [class.badge-info]="contest.status === 'RESOLVED'"
              >
                {{ statusLabels[contest.status] }}
              </div>
            </div>

            <p class="text-sm text-secondary mb-2">
              Contest creator resolves by selecting the winning Option; CLOSED
              is reserved for paused prediction windows.
            </p>

            <div class="grid grid-cols-2 gap-4 mb-4">
              <div>
                <p class="text-sm text-secondary">Created</p>
                <p class="font-semibold">
                  {{ contest.created_at | date: 'short' }}
                </p>
              </div>
              <div>
                <p class="text-sm text-secondary">Expires</p>
                <p class="font-semibold">
                  {{ contest.expires_at | date: 'short' }}
                </p>
              </div>
              <div>
                <p class="text-sm text-secondary">Total Staked</p>
                <p class="font-semibold">
                  💎 {{ getTotalClout(contest) }} Clout
                </p>
              </div>
              <div>
                <p class="text-sm text-secondary">Minimum Stake</p>
                <p class="font-semibold">💎 {{ contest.min_stake }} Clout</p>
              </div>
              <div>
                <p class="text-sm text-secondary">Predictions</p>
                <p class="font-semibold">{{ contest.predictions.length }}</p>
              </div>
              @if (contest.total_pot > 0) {
                <div>
                  <p class="text-sm text-secondary">Total Pot</p>
                  <p class="font-semibold text-lg">
                    💎 {{ contest.total_pot }} Clout
                  </p>
                </div>
                <div>
                  <p class="text-sm text-secondary">Clout Consumed (10%)</p>
                  <p class="font-semibold text-lg text-warning">
                    💎 {{ contest.clout_consumed }} Clout
                  </p>
                </div>
                <div>
                  <p class="text-sm text-secondary">Remaining Pool (90%)</p>
                  <p class="font-semibold text-lg text-success">
                    💎 {{ contest.total_pot - contest.clout_consumed }} Clout
                  </p>
                </div>
              }
            </div>

            @if (contest.status === 'RESOLVED' && contest.result_option_id) {
              <div class="alert alert-success mb-4">
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
                    d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z"
                  />
                </svg>
                <span
                  >Winner:
                  {{ getOptionText(contest, contest.result_option_id) }}</span
                >
              </div>
            }

            <div class="divider"></div>

            <h2 class="text-2xl font-bold mb-4">Options & Predictions</h2>

            <div class="space-y-4">
              @for (option of contest.options; track option.id) {
                <div class="card bg-base-200">
                  <div class="card-body">
                    <div class="flex justify-between items-center">
                      <h3 class="card-title">{{ option.text }}</h3>
                      @if (
                        contest.result_option_id === option.id &&
                        contest.status === 'RESOLVED'
                      ) {
                        <div class="badge badge-success">Winner 🎉</div>
                      }
                    </div>
                    <div class="stats stats-vertical lg:stats-horizontal">
                      <div class="stat">
                        <div class="stat-title">Predictions</div>
                        <div class="stat-value text-2xl">
                          {{ getPredictionsForOption(contest, option.id) }}
                        </div>
                      </div>
                      <div class="stat">
                        <div class="stat-title">Clout staked</div>
                        <div class="stat-value text-2xl">
                          {{ getCloutForOption(contest, option.id) }}
                        </div>
                      </div>
                    </div>
                  </div>
                </div>
              }
            </div>

            @if (contest.status === 'OPEN') {
              <div class="divider"></div>

              <h2 class="text-2xl font-bold mb-4">Make a Prediction</h2>
              <form (submit)="onSubmit($event)">
                <div class="alert alert-info mb-4">
                  <span>
                    Predictions are placed as
                    {{ auth.currentUser()?.username || 'your account' }}.
                  </span>
                </div>

                <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
                  <div class="form-control">
                    <label class="label">
                      <span class="label-text">Choose Option</span>
                    </label>
                    <select
                      class="select select-bordered"
                      [(ngModel)]="selectedOptionId"
                      name="optionId"
                      required
                    >
                      <option [ngValue]="null" disabled selected>
                        Select option
                      </option>
                      @for (option of contest.options; track option.id) {
                        <option [ngValue]="option.id">{{ option.text }}</option>
                      }
                    </select>
                  </div>

                  <div class="form-control">
                    <label class="label">
                      <span class="label-text">Clout Amount</span>
                    </label>
                    <input
                      type="number"
                      class="input input-bordered"
                      [(ngModel)]="cloutAmount"
                      name="clout"
                      [min]="contest.min_stake"
                      required
                    />
                    <p class="text-xs text-secondary mt-1">
                      Minimum stake: {{ contest.min_stake }} Clout
                    </p>
                  </div>
                </div>

                @if (predictionError()) {
                  <div class="alert alert-error mt-4">
                    <span>{{ predictionError() }}</span>
                  </div>
                }

                <div class="card-actions justify-end mt-4">
                  <button
                    type="submit"
                    class="btn btn-primary"
                    [disabled]="predictionLoading()"
                  >
                    @if (predictionLoading()) {
                      <span class="loading loading-spinner"></span>
                    }
                    Place Prediction
                  </button>
                </div>
              </form>
            }
          </div>
        </div>
      } @else {
        <div class="alert alert-error">
          <span>Contest not found</span>
        </div>
      }
    </div>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ContestDetailComponent implements OnInit {
  private readonly route = inject(ActivatedRoute);
  private readonly router = inject(Router);
  private readonly contestService = inject(ContestService);
  protected readonly auth = inject(AuthService);

  protected readonly contest = signal<Contest | null>(null);
  protected readonly loading = signal(true);
  protected readonly predictionLoading = signal(false);
  protected readonly predictionError = signal('');
  protected readonly statusLabels: Record<ContestStatus, string> = {
    OPEN: 'Open',
    CLOSED: 'Closed (paused)',
    RESOLVED: 'Resolved',
  };

  protected selectedOptionId: number | null = null;
  protected selectedCircleId: number | null = null;
  protected cloutAmount = 10;

  ngOnInit(): void {
    const id = Number(this.route.snapshot.paramMap.get('id'));
    const circleId = Number(this.route.snapshot.queryParamMap.get('circleId'));
    if (id) {
      this.loadContest(id);
      this.selectedCircleId = circleId;
    }
  }

  private loadContest(id: number): void {
    this.contestService.getContest(id).subscribe({
      next: (contest) => {
        this.contest.set(contest);
        this.cloutAmount = contest.min_stake;
        this.loading.set(false);
      },
      error: () => {
        this.loading.set(false);
      },
    });
  }

  protected onSubmit(event: Event): void {
    event.preventDefault();

    if (!this.selectedOptionId) {
      this.predictionError.set('Please select an option');
      return;
    }

    const contestId = this.contest()?.id;
    const contest = this.contest();
    if (!contestId || !contest) return;

    if (this.cloutAmount < contest.min_stake) {
      this.predictionError.set(
        `Minimum stake is ${contest.min_stake} Clout for this contest`,
      );
      return;
    }

    this.predictionLoading.set(true);
    this.predictionError.set('');

    this.contestService
      .makePrediction(contestId, {
        circle_id: this.selectedCircleId || 0,
        option_id: this.selectedOptionId,
        clout: this.cloutAmount,
      })
      .subscribe({
        next: () => {
          this.predictionLoading.set(false);
          // Reload contest to show new prediction
          this.loadContest(contestId);
          // Reset form
          this.selectedOptionId = null;
          this.cloutAmount = contest.min_stake;
        },
        error: (err) => {
          this.predictionLoading.set(false);
          this.predictionError.set(
            err.error?.error || 'Failed to place prediction',
          );
        },
      });
  }

  protected getTotalClout(contest: Contest): number {
    return contest.predictions.reduce((sum, pred) => sum + pred.clout, 0);
  }

  protected getPredictionsForOption(
    contest: Contest,
    optionId: number,
  ): number {
    return contest.predictions.filter((p) => p.option_id === optionId).length;
  }

  protected getCloutForOption(contest: Contest, optionId: number): number {
    return contest.predictions
      .filter((p) => p.option_id === optionId)
      .reduce((sum, pred) => sum + pred.clout, 0);
  }

  protected getOptionText(contest: Contest, optionId: number): string {
    return contest.options.find((o) => o.id === optionId)?.text || 'Unknown';
  }
}
