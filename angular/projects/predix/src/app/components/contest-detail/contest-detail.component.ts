import {
  ChangeDetectionStrategy,
  Component,
  signal,
  inject,
  OnInit,
  OnDestroy,
  computed,
} from '@angular/core';
import { ActivatedRoute } from '@angular/router';
import { Subscription } from 'rxjs';
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
  templateUrl: './contest-detail.component.html',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ContestDetailComponent implements OnInit, OnDestroy {
  private readonly route = inject(ActivatedRoute);
  private readonly contestService = inject(ContestService);
  public readonly auth = inject(AuthService);

  private pollingSubscription?: Subscription;

  public readonly contest = signal<Contest | null>(null);
  public readonly loading = signal(true);
  public readonly predictionLoading = signal(false);
  public readonly predictionError = signal('');
  public readonly optionStakes = signal<Record<number, number>>({});
  public readonly lockLoading = signal(false);
  public readonly resolveLoading = signal(false);
  public readonly resolveError = signal('');
  public readonly showResolveModal = signal(false);
  public readonly payoutBreakdown = signal<PayoutBreakdown | null>(null);
  public readonly payoutLoading = signal(false);
  public readonly payoutError = signal('');
  public readonly statusLabels: Record<ContestStatus, string> = {
    OPEN: 'Open',
    LOCKED: 'Locked (awaiting resolution)',
    CLOSED: 'Closed (paused)',
    RESOLVED: 'Resolved',
  };

  public readonly isCreator = computed(() => {
    const contest = this.contest();
    const user = this.auth.currentUser();
    return contest && user ? contest.creator_id === user.id : false;
  });

  public readonly canResolve = computed(() => {
    const contest = this.contest();
    return (
      this.isCreator() &&
      contest &&
      (contest.status === 'OPEN' ||
        contest.status === 'LOCKED' ||
        contest.status === 'CLOSED')
    );
  });

  public selectedWinnerId: number | null = null;

  ngOnInit(): void {
    const circleId = Number(this.route.snapshot.paramMap.get('circleId'));
    const contestId = Number(this.route.snapshot.paramMap.get('id'));
    if (circleId && contestId) {
      this.startPolling(circleId, contestId);
    }
  }

  ngOnDestroy(): void {
    this.pollingSubscription?.unsubscribe();
  }

  private startPolling(circleId: number, contestId: number): void {
    this.loading.set(true);

    this.pollingSubscription = this.contestService
      .pollContestDetails(circleId, contestId)
      .subscribe({
        next: (contestWithTotals) => {
          this.contest.set(contestWithTotals);
          this.syncOptionStakes(contestWithTotals);
          this.loading.set(false);

          // Load payout breakdown once if contest just became resolved
          if (
            contestWithTotals.status === 'RESOLVED' &&
            !this.payoutBreakdown()
          ) {
            this.loadPayoutBreakdown(circleId, contestId);
          }
        },
        error: (err) => {
          console.error('Polling failed after retries:', err);
          this.loading.set(false);
        },
        complete: () => {
          console.log('Polling stopped (contest closed or resolved)');
        },
      });
  }

  public loadContest(circleId: number, contestId: number): void {
    // Keep this method for explicit reloads after mutations
    this.contestService.getContest(circleId, contestId).subscribe({
      next: (contest) => {
        this.contest.set(contest);
        this.syncOptionStakes(contest);
        this.loading.set(false);

        // Load payout breakdown if contest is resolved
        if (contest.status === 'RESOLVED' && !this.payoutBreakdown()) {
          this.loadPayoutBreakdown(circleId, contestId);
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

  public loadPayoutBreakdown(circleId: number, contestId: number): void {
    this.payoutLoading.set(true);
    this.payoutError.set('');

    this.contestService.getPayoutBreakdown(circleId, contestId).subscribe({
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

  public getStakeForOption(optionId: number): number {
    const contest = this.contest();
    if (!contest) return 0;

    return this.optionStakes()[optionId] ?? contest.min_stake;
  }

  public setStake(optionId: number, value: number): void {
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

  public adjustStake(optionId: number, delta: number): void {
    const contest = this.contest();
    if (!contest) return;

    this.optionStakes.update((current) => {
      const currentValue = current[optionId] ?? contest.min_stake;
      const next = Math.max(contest.min_stake, currentValue + delta);
      return { ...current, [optionId]: next };
    });

    this.predictionError.set('');
  }

  public placePrediction(optionId: number): void {
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
      .makePrediction(contest.circle_id, contest.id, {
        option_id: optionId,
        clout: stake,
      })
      .subscribe({
        next: () => {
          this.predictionLoading.set(false);
          this.loadContest(contest.circle_id, contest.id);
        },
        error: (err) => {
          this.predictionLoading.set(false);
          this.predictionError.set(
            err.error?.error || 'Failed to place prediction',
          );
        },
      });
  }

  public onLock(): void {
    const contest = this.contest();
    if (!contest) return;

    this.lockLoading.set(true);

    this.contestService.lockContest(contest.circle_id, contest.id).subscribe({
      next: () => {
        this.lockLoading.set(false);
        // Reload contest to show updated status
        this.loadContest(contest.circle_id, contest.id);
      },
      error: () => {
        this.lockLoading.set(false);
      },
    });
  }

  public openResolveModal(): void {
    this.showResolveModal.set(true);
    this.selectedWinnerId = null;
    this.resolveError.set('');
  }

  public closeResolveModal(): void {
    this.showResolveModal.set(false);
    this.selectedWinnerId = null;
    this.resolveError.set('');
  }

  public onResolve(): void {
    if (!this.selectedWinnerId) {
      this.resolveError.set('Please select a winning option');
      return;
    }

    const contest = this.contest();
    if (!contest) return;

    this.resolveLoading.set(true);
    this.resolveError.set('');

    this.contestService
      .resolveContest(contest.circle_id, contest.id, {
        winning_option_id: this.selectedWinnerId,
      })
      .subscribe({
        next: () => {
          this.resolveLoading.set(false);
          this.closeResolveModal();
          // Reload contest to show resolved state and fetch payout breakdown
          this.loadContest(contest.circle_id, contest.id);
        },
        error: (err) => {
          this.resolveLoading.set(false);
          this.resolveError.set(
            err.error?.error || 'Failed to resolve contest',
          );
        },
      });
  }

  public getTotalClout(contest: Contest): number {
    return contest.predictions.reduce((sum, pred) => sum + pred.clout, 0);
  }

  public getPredictionsForOption(contest: Contest, optionId: number): number {
    return contest.predictions.filter((p) => p.option_id === optionId).length;
  }

  public getCloutForOption(contest: Contest, optionId: number): number {
    return contest.predictions
      .filter((p) => p.option_id === optionId)
      .reduce((sum, pred) => sum + pred.clout, 0);
  }

  public getOptionText(contest: Contest, optionId: number): string {
    return contest.options.find((o) => o.id === optionId)?.text || 'Unknown';
  }
}
