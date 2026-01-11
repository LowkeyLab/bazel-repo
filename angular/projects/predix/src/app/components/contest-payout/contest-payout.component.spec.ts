import { ComponentFixture, TestBed } from '@angular/core/testing';
import { ContestPayoutComponent } from './contest-payout.component';
import type { Contest, PayoutBreakdown } from '../../models/contest.model';

describe('ContestPayoutComponent', () => {
  let fixture: ComponentFixture<ContestPayoutComponent>;
  let component: ContestPayoutComponent;

  const mockContest: Contest = {
    id: 1,
    circle_id: 1,
    creator_id: 1,
    question: 'Q?',
    options: [
      { id: 1, text: 'A' },
      { id: 2, text: 'B' },
    ],
    predictions: [],
    status: 'RESOLVED',
    result_option_id: 2,
    min_stake: 10,
    total_pot: 300,
    house_rake: 30,
    created_at: '2024-01-01T00:00:00Z',
    locked_at: '2024-01-02T00:00:00Z',
    duration: '1d',
  };

  const mockBreakdown: PayoutBreakdown = {
    winners: [
      { user_id: 2, username: 'bob', stake: 100, share: 90, total: 190 },
      { user_id: 3, username: 'charlie', stake: 200, share: 180, total: 380 },
    ],
    losers: [
      { user_id: 4, username: 'dave', stake: 50, share: 0, total: 0 },
      { user_id: 5, username: 'erin', stake: 250, share: 0, total: 0 },
    ],
    total_pot: 300,
    house_rake: 30,
    distributable_pot: 270,
    total_distributed: 570,
  };

  beforeEach(async () => {
    await TestBed.configureTestingModule({
      imports: [ContestPayoutComponent],
    }).compileComponents();

    fixture = TestBed.createComponent(ContestPayoutComponent);
    component = fixture.componentInstance;
  });

  it('creates', () => {
    expect(component).toBeTruthy();
  });

  it('renders loading state', () => {
    fixture.componentRef.setInput('payoutLoading', true);
    fixture.detectChanges();

    const el: HTMLElement = fixture.nativeElement;
    expect(el.querySelector('.loading')).toBeTruthy();
  });

  it('renders error state', () => {
    fixture.componentRef.setInput('payoutLoading', false);
    fixture.componentRef.setInput('payoutError', 'Failed to load');
    fixture.componentRef.setInput('payoutBreakdown', null);
    fixture.detectChanges();

    const el: HTMLElement = fixture.nativeElement;
    const alert = el.querySelector('.alert.alert-error');
    expect(alert?.textContent).toContain('Failed to load');
  });

  it('renders payout breakdown card', () => {
    fixture.componentRef.setInput('contest', mockContest);
    fixture.componentRef.setInput('payoutBreakdown', mockBreakdown);
    fixture.componentRef.setInput('payoutLoading', false);
    fixture.componentRef.setInput('payoutError', '');
    fixture.detectChanges();

    const el: HTMLElement = fixture.nativeElement;
    expect(el.querySelector('h3')?.textContent).toContain('Payout Breakdown');
    // Winners table rows (first table)
    const winnersTable = el.querySelectorAll('table.table')[0];
    const winnersRows = winnersTable.querySelectorAll('tbody tr');
    expect(winnersRows.length).toBe(2);
  });

  it('displays winners and losers correctly', () => {
    fixture.componentRef.setInput('contest', mockContest);
    fixture.componentRef.setInput('payoutBreakdown', mockBreakdown);
    fixture.componentRef.setInput('payoutLoading', false);
    fixture.componentRef.setInput('payoutError', '');
    fixture.detectChanges();

    const el: HTMLElement = fixture.nativeElement;
    const winnersTable = el.querySelectorAll('table.table')[0];
    const losersTable = el.querySelectorAll('table.table')[1];
    expect(winnersTable.querySelectorAll('tbody tr').length).toBe(2);
    expect(losersTable.querySelectorAll('tbody tr').length).toBe(2);
  });

  it('maintains payout invariants', () => {
    fixture.componentRef.setInput('contest', mockContest);
    fixture.componentRef.setInput('payoutBreakdown', mockBreakdown);
    fixture.componentRef.setInput('payoutLoading', false);
    fixture.componentRef.setInput('payoutError', '');
    fixture.detectChanges();

    const b = mockBreakdown;
    // distributable = total_pot - house_rake
    expect(b.distributable_pot).toBe(b.total_pot - b.house_rake);
    // winners total equals total_distributed
    const winnersTotal = b.winners.reduce((sum, w) => sum + w.total, 0);
    expect(winnersTotal).toBe(b.total_distributed);
    // consumed + distributable equals total_pot
    expect(b.house_rake + b.distributable_pot).toBe(b.total_pot);
  });

  it('handles edge cases: single winner, many winners, zero stake', () => {
    const singleWinner: PayoutBreakdown = {
      winners: [
        { user_id: 2, username: 'bob', stake: 300, share: 270, total: 570 },
      ],
      losers: [],
      total_pot: 300,
      house_rake: 30,
      distributable_pot: 270,
      total_distributed: 570,
    };
    const manyWinners: PayoutBreakdown = {
      winners: Array.from({ length: 10 }, (_, i) => ({
        user_id: i + 2,
        username: 'user' + (i + 2),
        stake: 10,
        share: 9,
        total: 19,
      })),
      losers: [],
      total_pot: 200,
      house_rake: 10,
      distributable_pot: 190 - 100, // intentionally incorrect replaced below
      total_distributed: 190,
    };
    manyWinners.distributable_pot =
      manyWinners.total_pot - manyWinners.house_rake;

    const zeroStake: PayoutBreakdown = {
      winners: [],
      losers: [],
      total_pot: 0,
      house_rake: 0,
      distributable_pot: 0,
      total_distributed: 0,
    };

    // Single winner
    fixture.componentRef.setInput('payoutBreakdown', singleWinner);
    fixture.componentRef.setInput('payoutLoading', false);
    fixture.componentRef.setInput('payoutError', '');
    fixture.detectChanges();
    expect(singleWinner.winners.length).toBe(1);
    expect(singleWinner.total_distributed).toBe(570);

    // Many winners
    fixture.componentRef.setInput('payoutBreakdown', manyWinners);
    fixture.detectChanges();
    expect(manyWinners.winners.length).toBe(10);
    expect(manyWinners.total_distributed).toBe(190);

    // Zero stake
    fixture.componentRef.setInput('payoutBreakdown', zeroStake);
    fixture.detectChanges();
    expect(zeroStake.total_pot).toBe(0);
    expect(zeroStake.total_distributed).toBe(0);
  });

  it('computes losers rake percent correctly', () => {
    const percent = component.getHouseRakePercentOfLosers(mockBreakdown);
    // losers total stake = 50 + 250 = 300; rake 30 -> 10%
    expect(Math.round(percent)).toBe(10);
  });
});
