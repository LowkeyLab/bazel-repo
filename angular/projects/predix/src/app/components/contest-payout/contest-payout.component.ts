import { ChangeDetectionStrategy, Component, input } from '@angular/core';
import { DecimalPipe } from '@angular/common';
import type { Contest, PayoutBreakdown } from '../../models/contest.model';

@Component({
  selector: 'contest-payout',
  standalone: true,
  imports: [DecimalPipe],
  templateUrl: './contest-payout.component.html',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ContestPayoutComponent {
  // Inputs-only API
  contest = input<Contest | null>(null);
  payoutBreakdown = input<PayoutBreakdown | null>(null);
  payoutLoading = input<boolean>(false);
  payoutError = input<string>('');

  // Helper: used only within payout rendering
  public getHouseRakePercentOfLosers(
    breakdown: PayoutBreakdown | null,
  ): number {
    if (!breakdown) return 0;
    const losersTotal = breakdown.losers.reduce((sum, l) => sum + l.stake, 0);
    if (!losersTotal) return 0;
    return (breakdown.house_rake / losersTotal) * 100;
  }
}
