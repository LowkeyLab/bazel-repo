import {
  ChangeDetectionStrategy,
  Component,
  signal,
  inject,
  OnInit,
} from '@angular/core';
import { ActivatedRoute, Router } from '@angular/router';
import { ContestService } from '../../services/contest.service';
import type { Contest, ContestOption } from '../../models/contest.model';
import { DatePipe } from '@angular/common';
import { FormsModule } from '@angular/forms';
import { AuthService } from '../../services/auth.service';

@Component({
  selector: 'app-contest-detail',
  imports: [DatePipe, FormsModule],
  template: `
    <div class="container mx-auto px-4 py-8">
      @if (loading()) {
        <div class="flex justify-center">
          <span class="loading loading-spinner loading-lg"></span>
        </div>
      } @else if (contest(); as contest) {
        <div class="mb-6">
          <button class="btn btn-ghost btn-sm" (click)="goBack()">
            ← Back
          </button>
        </div>

        <div class="card bg-base-100 shadow-xl mb-6">
          <div class="card-body">
            <div class="flex justify-between items-start mb-4">
              <h1 class="text-3xl font-bold flex-1">{{ contest.question }}</h1>
              <div
                class="badge badge-lg"
                [class.badge-success]="contest.status === 'OPEN'"
                [class.badge-warning]="contest.status === 'CLOSED'"
                [class.badge-info]="contest.status === 'RESOLVED'"
              >
                {{ contest.status }}
              </div>
            </div>

            <div class="grid grid-cols-2 gap-4 mb-4">
              <div>
                <p class="text-sm text-gray-500">Created</p>
                <p class="font-semibold">
                  {{ contest.created_at | date: 'short' }}
                </p>
              </div>
              <div>
                <p class="text-sm text-gray-500">Expires</p>
                <p class="font-semibold">
                  {{ contest.expires_at | date: 'short' }}
                </p>
              </div>
              <div>
                <p class="text-sm text-gray-500">Total Pool</p>
                <p class="font-semibold">{{ getTotalClout(contest) }} clout</p>
              </div>
              <div>
                <p class="text-sm text-gray-500">Predictions</p>
                <p class="font-semibold">{{ contest.predictions.length }}</p>
              </div>
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
                        <div class="stat-title">Total Clout</div>
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
                      min="1"
                      required
                    />
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

  protected selectedOptionId: number | null = null;
  protected cloutAmount = 10;

  ngOnInit(): void {
    const id = Number(this.route.snapshot.paramMap.get('id'));
    if (id) {
      this.loadContest(id);
    }
  }

  private loadContest(id: number): void {
    this.contestService.getContest(id).subscribe({
      next: (contest) => {
        this.contest.set(contest);
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
    if (!contestId) return;

    this.predictionLoading.set(true);
    this.predictionError.set('');

    this.contestService
      .makePrediction(contestId, {
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
          this.cloutAmount = 10;
        },
        error: (err) => {
          this.predictionLoading.set(false);
          this.predictionError.set(
            err.error?.error || 'Failed to place prediction',
          );
        },
      });
  }

  protected goBack(): void {
    this.router.navigate(['/contests']);
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
