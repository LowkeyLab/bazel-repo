import {
  ChangeDetectionStrategy,
  Component,
  signal,
  inject,
  OnInit,
  computed,
} from '@angular/core';
import { ActivatedRoute, Router } from '@angular/router';
import { ContestService } from '../../services/contest.service';
import type {
  Contest,
  ContestStatus,
  PayoutBreakdown,
} from '../../models/contest.model';
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
                [class.badge-warning]="contest.status === 'LOCKED'"
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
                  {{ contest.closes_at | date: 'short' }}
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

              <!-- Payout Breakdown Section -->
              @if (payoutLoading()) {
                <div class="card bg-base-200 mb-4">
                  <div class="card-body">
                    <div class="flex justify-center">
                      <span class="loading loading-spinner"></span>
                    </div>
                  </div>
                </div>
              } @else if (payoutBreakdown(); as breakdown) {
                <div class="card bg-base-200 mb-4">
                  <div class="card-body">
                    <h3 class="text-xl font-bold mb-2">💰 Payout Breakdown</h3>
                    <p class="text-sm text-secondary mb-4">
                      Winners receive their original stake plus their
                      proportional share of the remaining pot (90% after 10%
                      consumption).
                    </p>
                    <div class="grid grid-cols-2 gap-4 mb-6">
                      <div>
                        <p class="text-sm text-secondary">Total Pot</p>
                        <p class="font-semibold text-lg">
                          💎 {{ breakdown.total_pot }} Clout
                        </p>
                      </div>
                      <div>
                        <p class="text-sm text-secondary">
                          Clout Consumed (10%)
                        </p>
                        <p class="font-semibold text-lg text-warning">
                          💎 {{ breakdown.clout_consumed }} Clout
                        </p>
                      </div>
                      <div>
                        <p class="text-sm text-secondary">
                          Distributable (90%)
                        </p>
                        <p class="font-semibold text-lg text-success">
                          💎 {{ breakdown.distributable_pot }} Clout
                        </p>
                      </div>
                      <div>
                        <p class="text-sm text-secondary">Total Distributed</p>
                        <p class="font-semibold text-lg text-info">
                          💎 {{ breakdown.total_distributed }} Clout
                        </p>
                      </div>
                    </div>
                    <div class="overflow-x-auto">
                      <table class="table table-zebra">
                        <thead>
                          <tr>
                            <th>User ID</th>
                            <th>Original Stake</th>
                            <th>Share of Pot</th>
                            <th>Total Payout</th>
                          </tr>
                        </thead>
                        <tbody>
                          @for (
                            payout of breakdown.winners;
                            track payout.user_id
                          ) {
                            <tr>
                              <td>{{ payout.user_id }}</td>
                              <td>💎 {{ payout.stake }}</td>
                              <td>💎 {{ payout.share }}</td>
                              <td class="font-bold text-success">
                                💎 {{ payout.total }}
                              </td>
                            </tr>
                          }
                        </tbody>
                      </table>
                    </div>
                  </div>
                </div>
              } @else if (payoutError()) {
                <div class="alert alert-error mb-4">
                  <span>{{ payoutError() }}</span>
                </div>
              }
            }

            @if (canResolve()) {
              <div class="alert alert-warning mb-4">
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
                    d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
                  />
                </svg>
                <div class="flex-1">
                  <span
                    >As the creator, you can resolve this contest by selecting
                    the winning option.</span
                  >
                </div>
                <button
                  type="button"
                  class="btn btn-sm btn-warning"
                  (click)="openResolveModal()"
                >
                  🏆 Resolve Contest
                </button>
              </div>
            }

            @if (contest.status === 'OPEN' && isCreator()) {
              <div class="alert alert-info mb-4">
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
                    d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
                  />
                </svg>
                <div class="flex-1">
                  <span
                    >As the creator, you can lock this contest to prevent
                    further predictions.</span
                  >
                </div>
                <button
                  type="button"
                  class="btn btn-sm btn-warning"
                  (click)="onLock()"
                  [disabled]="lockLoading()"
                >
                  @if (lockLoading()) {
                    <span class="loading loading-spinner"></span>
                  } @else {
                    🔒 Lock Predictions
                  }
                </button>
              </div>
            }

            <div class="divider"></div>

            <h2 class="text-2xl font-bold mb-4">Options & Predictions</h2>

            @if (contest.status === 'OPEN') {
              <div class="alert alert-info mb-4">
                <span>
                  Adjust stakes per option. Submitting again updates your
                  existing stake for that option.
                </span>
              </div>

              @if (predictionError()) {
                <div class="alert alert-error mb-4">
                  <span>{{ predictionError() }}</span>
                </div>
              }
            }

            <div class="grid gap-4 md:grid-cols-2 xl:grid-cols-3">
              @for (option of contest.options; track option.id) {
                <div class="card bg-base-200 h-full">
                  <div class="card-body flex flex-col gap-4">
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

                    @if (contest.status === 'OPEN') {
                      <div class="divider my-0"></div>
                      <div class="flex flex-col gap-3">
                        <div class="flex items-center gap-2 flex-wrap">
                          <span class="text-sm text-secondary">Your stake</span>
                          <input
                            type="number"
                            class="input input-bordered input-sm w-32"
                            [value]="getStakeForOption(option.id)"
                            (input)="
                              setStake(
                                option.id,
                                $any($event.target).valueAsNumber
                              )
                            "
                            [min]="contest.min_stake"
                            [disabled]="predictionLoading()"
                          />
                          <span class="text-xs text-secondary">
                            Min {{ contest.min_stake }} Clout
                          </span>
                        </div>

                        <div class="flex flex-wrap gap-2">
                          <button
                            type="button"
                            class="btn btn-outline btn-sm"
                            (click)="adjustStake(option.id, -100)"
                            [disabled]="
                              predictionLoading() ||
                              getStakeForOption(option.id) <= contest.min_stake
                            "
                          >
                            -100
                          </button>
                          <button
                            type="button"
                            class="btn btn-outline btn-sm"
                            (click)="adjustStake(option.id, -1000)"
                            [disabled]="
                              predictionLoading() ||
                              getStakeForOption(option.id) <= contest.min_stake
                            "
                          >
                            -1000
                          </button>
                          <button
                            type="button"
                            class="btn btn-outline btn-sm"
                            (click)="adjustStake(option.id, -10000)"
                            [disabled]="
                              predictionLoading() ||
                              getStakeForOption(option.id) <= contest.min_stake
                            "
                          >
                            -10000
                          </button>
                          <div class="flex-1"></div>
                          <button
                            type="button"
                            class="btn btn-outline btn-sm"
                            (click)="adjustStake(option.id, 100)"
                            [disabled]="predictionLoading()"
                          >
                            +100
                          </button>
                          <button
                            type="button"
                            class="btn btn-outline btn-sm"
                            (click)="adjustStake(option.id, 1000)"
                            [disabled]="predictionLoading()"
                          >
                            +1000
                          </button>
                          <button
                            type="button"
                            class="btn btn-outline btn-sm"
                            (click)="adjustStake(option.id, 10000)"
                            [disabled]="predictionLoading()"
                          >
                            +10000
                          </button>
                        </div>

                        <div class="flex justify-end">
                          <button
                            type="button"
                            class="btn btn-primary btn-sm"
                            (click)="placePrediction(option.id)"
                            [disabled]="predictionLoading()"
                          >
                            @if (predictionLoading()) {
                              <span class="loading loading-spinner"></span>
                            }
                            Place / Update Prediction
                          </button>
                        </div>
                      </div>
                    }
                  </div>
                </div>
              }
            </div>
          </div>
        </div>
      } @else {
        <div class="alert alert-error">
          <span>Contest not found</span>
        </div>
      }

      <!-- Resolve Contest Modal -->
      @if (showResolveModal() && contest(); as contest) {
        <div class="modal modal-open">
          <div class="modal-box">
            <h3 class="font-bold text-lg mb-4">🏆 Resolve Contest</h3>
            <p class="text-sm text-secondary mb-4">
              Select the winning option. This action is final and cannot be
              undone. Winners will receive their stake plus their proportional
              share of 90% of the pot (10% is consumed as a house fee).
            </p>

            <div class="form-control mb-4">
              <label class="label">
                <span class="label-text">Winning Option</span>
              </label>
              <select
                class="select select-bordered"
                [(ngModel)]="selectedWinnerId"
                name="winnerId"
                required
              >
                <option [ngValue]="null" disabled selected>
                  Select winning option
                </option>
                @for (option of contest.options; track option.id) {
                  <option [ngValue]="option.id">
                    {{ option.text }} ({{
                      getPredictionsForOption(contest, option.id)
                    }}
                    predictions, 💎
                    {{ getCloutForOption(contest, option.id) }} staked)
                  </option>
                }
              </select>
            </div>

            @if (resolveError()) {
              <div class="alert alert-error mb-4">
                <span>{{ resolveError() }}</span>
              </div>
            }

            <div class="modal-action">
              <button
                type="button"
                class="btn"
                (click)="closeResolveModal()"
                [disabled]="resolveLoading()"
              >
                Cancel
              </button>
              <button
                type="button"
                class="btn btn-primary"
                (click)="onResolve()"
                [disabled]="resolveLoading() || !selectedWinnerId"
              >
                @if (resolveLoading()) {
                  <span class="loading loading-spinner"></span>
                }
                Confirm & Resolve
              </button>
            </div>
          </div>
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
  protected readonly optionStakes = signal<Record<number, number>>({});
  protected readonly lockLoading = signal(false);
  protected readonly resolveLoading = signal(false);
  protected readonly resolveError = signal('');
  protected readonly showResolveModal = signal(false);
  protected readonly payoutBreakdown = signal<PayoutBreakdown | null>(null);
  protected readonly payoutLoading = signal(false);
  protected readonly payoutError = signal('');
  protected readonly statusLabels: Record<ContestStatus, string> = {
    OPEN: 'Open',
    LOCKED: 'Locked (awaiting resolution)',
    CLOSED: 'Closed (paused)',
    RESOLVED: 'Resolved',
  };

  protected readonly isCreator = computed(() => {
    const contest = this.contest();
    const user = this.auth.currentUser();
    return contest && user ? contest.creator_id === user.id : false;
  });

  protected readonly canResolve = computed(() => {
    const contest = this.contest();
    return (
      this.isCreator() &&
      contest &&
      (contest.status === 'OPEN' ||
        contest.status === 'LOCKED' ||
        contest.status === 'CLOSED')
    );
  });

  protected selectedWinnerId: number | null = null;

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
        this.syncOptionStakes(contest);
        this.loading.set(false);

        // Load payout breakdown if contest is resolved
        if (contest.status === 'RESOLVED') {
          this.loadPayoutBreakdown(id);
        }
      },
      error: () => {
        this.loading.set(false);
      },
    });
  }

  private syncOptionStakes(contest: Contest): void {
    const stakes: Record<number, number> = {};
    const userId = this.auth.currentUser()?.id;

    for (const option of contest.options) {
      const existing = contest.predictions.find(
        (p) => p.option_id === option.id && p.user_id === userId,
      );
      stakes[option.id] = existing?.clout ?? contest.min_stake;
    }

    this.optionStakes.set(stakes);
  }

  private loadPayoutBreakdown(contestId: number): void {
    this.payoutLoading.set(true);
    this.payoutError.set('');

    this.contestService.getPayoutBreakdown(contestId).subscribe({
      next: (breakdown) => {
        this.payoutBreakdown.set(breakdown);
        this.payoutLoading.set(false);
      },
      error: (err) => {
        this.payoutError.set(
          err.error?.error || 'Failed to load payout breakdown',
        );
        this.payoutLoading.set(false);
      },
    });
  }

  protected getStakeForOption(optionId: number): number {
    const contest = this.contest();
    if (!contest) return 0;

    return this.optionStakes()[optionId] ?? contest.min_stake;
  }

  protected setStake(optionId: number, value: number): void {
    const contest = this.contest();
    if (!contest) return;

    const parsed = Number.isFinite(value) ? value : contest.min_stake;
    const next = Math.max(contest.min_stake, parsed);

    this.optionStakes.update((current) => ({
      ...current,
      [optionId]: next,
    }));
    this.predictionError.set('');
  }

  protected adjustStake(optionId: number, delta: number): void {
    const contest = this.contest();
    if (!contest) return;

    this.optionStakes.update((current) => {
      const currentValue = current[optionId] ?? contest.min_stake;
      const next = Math.max(contest.min_stake, currentValue + delta);
      return { ...current, [optionId]: next };
    });

    this.predictionError.set('');
  }

  protected placePrediction(optionId: number): void {
    const contest = this.contest();
    if (!contest) return;

    const stake = this.getStakeForOption(optionId);
    if (stake < contest.min_stake) {
      this.predictionError.set(
        `Minimum stake is ${contest.min_stake} Clout for this contest`,
      );
      return;
    }

    this.predictionLoading.set(true);
    this.predictionError.set('');

    this.contestService
      .makePrediction(contest.id, {
        option_id: optionId,
        clout: stake,
      })
      .subscribe({
        next: () => {
          this.predictionLoading.set(false);
          this.loadContest(contest.id);
        },
        error: (err) => {
          this.predictionLoading.set(false);
          this.predictionError.set(
            err.error?.error || 'Failed to place prediction',
          );
        },
      });
  }

  protected onLock(): void {
    const contestId = this.contest()?.id;
    if (!contestId) return;

    this.lockLoading.set(true);

    this.contestService.lockContest(contestId).subscribe({
      next: () => {
        this.lockLoading.set(false);
        // Reload contest to show updated status
        this.loadContest(contestId);
      },
      error: () => {
        this.lockLoading.set(false);
      },
    });
  }

  protected openResolveModal(): void {
    this.showResolveModal.set(true);
    this.selectedWinnerId = null;
    this.resolveError.set('');
  }

  protected closeResolveModal(): void {
    this.showResolveModal.set(false);
    this.selectedWinnerId = null;
    this.resolveError.set('');
  }

  protected onResolve(): void {
    if (!this.selectedWinnerId) {
      this.resolveError.set('Please select a winning option');
      return;
    }

    const contestId = this.contest()?.id;
    if (!contestId) return;

    this.resolveLoading.set(true);
    this.resolveError.set('');

    this.contestService
      .resolveContest(contestId, {
        winning_option_id: this.selectedWinnerId,
      })
      .subscribe({
        next: () => {
          this.resolveLoading.set(false);
          this.closeResolveModal();
          // Reload contest to show resolved state and fetch payout breakdown
          this.loadContest(contestId);
        },
        error: (err) => {
          this.resolveLoading.set(false);
          this.resolveError.set(
            err.error?.error || 'Failed to resolve contest',
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
