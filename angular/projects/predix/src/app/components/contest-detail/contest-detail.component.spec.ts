import { ComponentFixture, TestBed } from '@angular/core/testing';
import { ContestDetailComponent } from './contest-detail.component';
import { ActivatedRoute, Router } from '@angular/router';
import { ContestService } from '../../services/contest.service';
import { AuthService } from '../../services/auth.service';
import { of, throwError, Subject } from 'rxjs';
import type { Contest, PayoutBreakdown } from '../../models/contest.model';

describe('ContestDetailComponent - Payout Breakdown', () => {
  let component: ContestDetailComponent;
  let fixture: ComponentFixture<ContestDetailComponent>;
  let mockContestService: jasmine.SpyObj<ContestService>;
  let mockAuthService: jasmine.SpyObj<AuthService>;
  let mockRouter: jasmine.SpyObj<Router>;
  let mockActivatedRoute: Partial<ActivatedRoute>;

  const mockResolvedContest: Contest = {
    id: 1,
    circle_id: 1,
    creator_id: 1,
    question: 'What is 2+2?',
    options: [
      { id: 1, text: '3' },
      { id: 2, text: '4' },
    ],
    predictions: [
      {
        user_id: 2,
        option_id: 2,
        clout: 100,
        timestamp: '2024-01-01T12:00:00Z',
      },
      {
        user_id: 3,
        option_id: 2,
        clout: 200,
        timestamp: '2024-01-01T13:00:00Z',
      },
    ],
    status: 'RESOLVED',
    result_option_id: 2,
    min_stake: 10,
    total_pot: 300,
    house_rake: 30,
    created_at: '2024-01-01T00:00:00Z',
    closes_at: '2024-01-02T00:00:00Z',
    duration: '1d',
  };

  const mockPayoutBreakdown: PayoutBreakdown = {
    winners: [
      {
        user_id: 2,
        username: 'bob',
        stake: 100,
        share: 90,
        total: 190,
      },
      {
        user_id: 3,
        username: 'charlie',
        stake: 200,
        share: 180,
        total: 380,
      },
    ],
    losers: [],
    total_pot: 300,
    house_rake: 30,
    distributable_pot: 270,
    total_distributed: 570,
  };

  const mockOpenContest: Contest = {
    ...mockResolvedContest,
    status: 'OPEN',
    result_option_id: undefined,
  };

  beforeEach(async () => {
    mockContestService = jasmine.createSpyObj('ContestService', [
      'getContest',
      'makePrediction',
      'lockContest',
      'resolveContest',
      'getPayoutBreakdown',
      'pollContestDetails',
    ]);
    mockAuthService = jasmine.createSpyObj('AuthService', ['currentUser']);
    mockRouter = jasmine.createSpyObj('Router', ['navigate']);
    mockActivatedRoute = {
      snapshot: {
        paramMap: {
          get: jasmine.createSpy('get').and.callFake((key: string) => {
            if (key === 'id') return '1';
            if (key === 'circleId') return '1';
            return null;
          }),
        },
      } as any,
    };

    mockAuthService.currentUser.and.returnValue({
      id: 1,
      username: 'alice',
      role: 'member',
    });

    await TestBed.configureTestingModule({
      imports: [ContestDetailComponent],
      providers: [
        { provide: ContestService, useValue: mockContestService },
        { provide: AuthService, useValue: mockAuthService },
        { provide: Router, useValue: mockRouter },
        { provide: ActivatedRoute, useValue: mockActivatedRoute },
      ],
    }).compileComponents();

    // Default mock for pollContestDetails (can be overridden in specific tests)
    mockContestService.pollContestDetails.and.returnValue(
      of({
        ...mockOpenContest,
        totals: { byOption: new Map() },
      }),
    );
    mockContestService.getContest.and.returnValue(of(mockOpenContest));

    fixture = TestBed.createComponent(ContestDetailComponent);
    component = fixture.componentInstance;
  });

  it('should create', () => {
    expect(component).toBeTruthy();
  });

  describe('Payout Breakdown Loading', () => {
    it('should load payout breakdown when contest is RESOLVED', () => {
      const contestWithTotals = {
        ...mockResolvedContest,
        totals: {
          byOption: new Map([
            [1, { clout: 0, count: 0 }],
            [2, { clout: 300, count: 2 }],
          ]),
        },
      };
      mockContestService.pollContestDetails.and.returnValue(
        of(contestWithTotals),
      );
      mockContestService.getPayoutBreakdown.and.returnValue(
        of(mockPayoutBreakdown),
      );

      fixture.detectChanges();

      expect(mockContestService.pollContestDetails).toHaveBeenCalledWith(1, 1);
      expect(mockContestService.getPayoutBreakdown).toHaveBeenCalledWith(1, 1);
      expect(component.payoutBreakdown()).toEqual(mockPayoutBreakdown);
    });

    it('should not load payout breakdown when contest is OPEN', () => {
      const contestWithTotals = {
        ...mockOpenContest,
        totals: { byOption: new Map() },
      };
      mockContestService.pollContestDetails.and.returnValue(
        of(contestWithTotals),
      );

      fixture.detectChanges();

      expect(mockContestService.pollContestDetails).toHaveBeenCalledWith(1, 1);
      expect(mockContestService.getPayoutBreakdown).not.toHaveBeenCalled();
    });

    it('should set payoutLoading signal to true while fetching', (done) => {
      const contestWithTotals = {
        ...mockResolvedContest,
        totals: { byOption: new Map() },
      };
      mockContestService.pollContestDetails.and.returnValue(
        of(contestWithTotals),
      );
      mockContestService.getPayoutBreakdown.and.returnValue(
        of(mockPayoutBreakdown),
      );

      fixture.detectChanges();

      // Check that payoutLoading is false after loading completes
      setTimeout(() => {
        expect(component.payoutLoading()).toBe(false);
        done();
      }, 0);
    });

    it('should handle payout breakdown fetch error', () => {
      mockContestService.pollContestDetails.and.returnValue(
        of({ ...mockResolvedContest, totals: { byOption: new Map() } }),
      );
      mockContestService.getPayoutBreakdown.and.returnValue(
        throwError(() => ({
          error: { error: 'Failed to load payout breakdown' },
        })),
      );

      fixture.detectChanges();

      expect(component.payoutError()).toBe('Failed to load payout breakdown');
      expect(component.payoutLoading()).toBe(false);
    });

    it('should handle generic payout breakdown fetch error', () => {
      mockContestService.pollContestDetails.and.returnValue(
        of({ ...mockResolvedContest, totals: { byOption: new Map() } }),
      );
      mockContestService.getPayoutBreakdown.and.returnValue(
        throwError(() => new Error('Network error')),
      );

      fixture.detectChanges();

      expect(component.payoutError()).toBe('Failed to load payout breakdown');
      expect(component.payoutLoading()).toBe(false);
    });
  });

  describe('Payout Breakdown Display', () => {
    beforeEach(() => {
      mockContestService.pollContestDetails.and.returnValue(
        of({ ...mockResolvedContest, totals: { byOption: new Map() } }),
      );
      mockContestService.getPayoutBreakdown.and.returnValue(
        of(mockPayoutBreakdown),
      );
      fixture.detectChanges();
    });

    it('should display payout breakdown data correctly', () => {
      const breakdown = component.payoutBreakdown();
      expect(breakdown).toBeTruthy();
      expect(breakdown?.total_pot).toBe(300);
      expect(breakdown?.house_rake).toBe(30);
      expect(breakdown?.distributable_pot).toBe(270);
      expect(breakdown?.total_distributed).toBe(570);
    });

    it('should display all winners in payout breakdown', () => {
      const breakdown = component.payoutBreakdown();
      expect(breakdown?.winners.length).toBe(2);
      expect(breakdown?.winners[0]).toEqual({
        user_id: 2,
        username: 'bob',
        stake: 100,
        share: 90,
        total: 190,
      });
      expect(breakdown?.winners[1]).toEqual({
        user_id: 3,
        username: 'charlie',
        stake: 200,
        share: 180,
        total: 380,
      });
    });

    it('should calculate payout totals correctly', () => {
      const breakdown = component.payoutBreakdown();
      expect(breakdown).toBeTruthy();
      const actualTotal = breakdown!.winners.reduce(
        (sum, winner) => sum + winner.total,
        0,
      );
      expect(actualTotal).toBe(breakdown!.total_distributed);
    });

    it('should verify payout breakdown totals sum correctly', () => {
      const breakdown = component.payoutBreakdown();
      expect(breakdown).toBeTruthy();
      expect(breakdown!.total_distributed).toBe(570);
      // Total pot - house rake = distributable
      expect(breakdown!.total_pot - breakdown!.house_rake).toBe(
        breakdown!.distributable_pot,
      );
    });
  });

  describe('Payout Breakdown Not Shown for Non-Resolved Contests', () => {
    it('should not load payout breakdown when contest status is OPEN', () => {
      mockContestService.pollContestDetails.and.returnValue(
        of({ ...mockOpenContest, totals: { byOption: new Map() } }),
      );

      fixture.detectChanges();

      expect(component.payoutBreakdown()).toBeNull();
      expect(mockContestService.getPayoutBreakdown).not.toHaveBeenCalled();
    });

    it('should not load payout breakdown when contest status is LOCKED', () => {
      const lockedContest: Contest = {
        ...mockResolvedContest,
        status: 'LOCKED',
        result_option_id: undefined,
      };
      mockContestService.pollContestDetails.and.returnValue(
        of({ ...lockedContest, totals: { byOption: new Map() } }),
      );

      fixture.detectChanges();

      expect(component.payoutBreakdown()).toBeNull();
      expect(mockContestService.getPayoutBreakdown).not.toHaveBeenCalled();
    });

    it('should not load payout breakdown when contest status is CLOSED', () => {
      const closedContest: Contest = {
        ...mockResolvedContest,
        status: 'CLOSED',
        result_option_id: undefined,
      };
      mockContestService.pollContestDetails.and.returnValue(
        of({ ...closedContest, totals: { byOption: new Map() } }),
      );

      fixture.detectChanges();

      expect(component.payoutBreakdown()).toBeNull();
      expect(mockContestService.getPayoutBreakdown).not.toHaveBeenCalled();
    });
  });

  describe('Contest Resolution Flow', () => {
    it('should load payout breakdown after resolving contest', () => {
      mockContestService.pollContestDetails.and.returnValue(
        of({ ...mockOpenContest, totals: { byOption: new Map() } }),
      );

      fixture.detectChanges();
      expect(mockContestService.getPayoutBreakdown).not.toHaveBeenCalled();

      // Simulate resolution
      mockContestService.pollContestDetails.and.returnValue(
        of({ ...mockResolvedContest, totals: { byOption: new Map() } }),
      );
      mockContestService.getContest.and.returnValue(of(mockResolvedContest)); // Add getContest mock for loadContest method
      mockContestService.getPayoutBreakdown.and.returnValue(
        of(mockPayoutBreakdown),
      );

      component.loadContest(1, 1);

      expect(mockContestService.getPayoutBreakdown).toHaveBeenCalledWith(1, 1);
      expect(component.payoutBreakdown()).toEqual(mockPayoutBreakdown);
    });

    it('should handle error when loading payout breakdown after resolution', () => {
      mockContestService.pollContestDetails.and.returnValue(
        of({ ...mockResolvedContest, totals: { byOption: new Map() } }),
      );
      mockContestService.getPayoutBreakdown.and.returnValue(
        throwError(() => ({
          error: { error: 'Contest not found' },
        })),
      );

      fixture.detectChanges();

      expect(component.payoutError()).toBe('Contest not found');
      expect(component.payoutBreakdown()).toBeNull();
    });
  });

  describe('Payout Breakdown Signals', () => {
    it('should initialize payoutBreakdown signal as null', () => {
      expect(component.payoutBreakdown()).toBeNull();
    });

    it('should initialize payoutLoading signal as false', () => {
      expect(component.payoutLoading()).toBe(false);
    });

    it('should initialize payoutError signal as empty string', () => {
      expect(component.payoutError()).toBe('');
    });

    it('should update all three signals when loading succeeds', () => {
      mockContestService.pollContestDetails.and.returnValue(
        of({ ...mockResolvedContest, totals: { byOption: new Map() } }),
      );
      mockContestService.getPayoutBreakdown.and.returnValue(
        of(mockPayoutBreakdown),
      );

      expect(component.payoutBreakdown()).toBeNull();
      expect(component.payoutLoading()).toBe(false);
      expect(component.payoutError()).toBe('');

      fixture.detectChanges();

      expect(component.payoutBreakdown()).toEqual(mockPayoutBreakdown);
      expect(component.payoutLoading()).toBe(false);
      expect(component.payoutError()).toBe('');
    });

    it('should update signals correctly when loading fails', () => {
      const errorMessage = 'Connection timeout';
      mockContestService.pollContestDetails.and.returnValue(
        of({ ...mockResolvedContest, totals: { byOption: new Map() } }),
      );
      mockContestService.getPayoutBreakdown.and.returnValue(
        throwError(() => ({
          error: { error: errorMessage },
        })),
      );

      fixture.detectChanges();

      expect(component.payoutBreakdown()).toBeNull();
      expect(component.payoutLoading()).toBe(false);
      expect(component.payoutError()).toBe(errorMessage);
    });
  });

  describe('Payout Breakdown Edge Cases', () => {
    it('should handle contest with single winner', () => {
      const singleWinnerBreakdown: PayoutBreakdown = {
        winners: [
          {
            user_id: 2,
            username: 'bob',
            stake: 300,
            share: 270,
            total: 570,
          },
        ],
        losers: [],
        total_pot: 300,
        house_rake: 30,
        distributable_pot: 270,
        total_distributed: 570,
      };

      mockContestService.pollContestDetails.and.returnValue(
        of({ ...mockResolvedContest, totals: { byOption: new Map() } }),
      );
      mockContestService.getPayoutBreakdown.and.returnValue(
        of(singleWinnerBreakdown),
      );

      fixture.detectChanges();

      const breakdown = component.payoutBreakdown();
      expect(breakdown?.winners.length).toBe(1);
      expect(breakdown?.total_distributed).toBe(570);
    });

    it('should handle contest with many winners', () => {
      const manyWinnersBreakdown: PayoutBreakdown = {
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
        distributable_pot: 90,
        total_distributed: 190,
      };

      mockContestService.pollContestDetails.and.returnValue(
        of({ ...mockResolvedContest, totals: { byOption: new Map() } }),
      );
      mockContestService.getPayoutBreakdown.and.returnValue(
        of(manyWinnersBreakdown),
      );

      fixture.detectChanges();

      const breakdown = component.payoutBreakdown();
      expect(breakdown?.winners.length).toBe(10);
      expect(breakdown?.total_distributed).toBe(190);
    });

    it('should handle zero clout stake', () => {
      const zeroStakeBreakdown: PayoutBreakdown = {
        winners: [],
        losers: [],
        total_pot: 0,
        house_rake: 0,
        distributable_pot: 0,
        total_distributed: 0,
      };

      mockContestService.pollContestDetails.and.returnValue(
        of({ ...mockResolvedContest, totals: { byOption: new Map() } }),
      );
      mockContestService.getPayoutBreakdown.and.returnValue(
        of(zeroStakeBreakdown),
      );

      fixture.detectChanges();

      const breakdown = component.payoutBreakdown();
      expect(breakdown?.total_pot).toBe(0);
      expect(breakdown?.total_distributed).toBe(0);
    });
  });

  describe('House Rake Verification', () => {
    it('should verify 10% house rake is applied correctly', () => {
      mockContestService.pollContestDetails.and.returnValue(
        of({ ...mockResolvedContest, totals: { byOption: new Map() } }),
      );
      mockContestService.getPayoutBreakdown.and.returnValue(
        of(mockPayoutBreakdown),
      );

      fixture.detectChanges();

      const breakdown = component.payoutBreakdown();
      expect(breakdown).toBeTruthy();
      const expectedConsumed = Math.floor(breakdown!.total_pot * 0.1);
      expect(breakdown!.house_rake).toBe(expectedConsumed);
    });

    it('should verify 90% distributable rate is applied correctly', () => {
      mockContestService.pollContestDetails.and.returnValue(
        of({ ...mockResolvedContest, totals: { byOption: new Map() } }),
      );
      mockContestService.getPayoutBreakdown.and.returnValue(
        of(mockPayoutBreakdown),
      );

      fixture.detectChanges();

      const breakdown = component.payoutBreakdown();
      expect(breakdown).toBeTruthy();
      const expectedDistributable =
        breakdown!.total_pot - breakdown!.house_rake;
      expect(breakdown!.distributable_pot).toBe(expectedDistributable);
    });

    it('should maintain invariant: consumed + distributable = total_pot', () => {
      mockContestService.pollContestDetails.and.returnValue(
        of({ ...mockResolvedContest, totals: { byOption: new Map() } }),
      );
      mockContestService.getPayoutBreakdown.and.returnValue(
        of(mockPayoutBreakdown),
      );

      fixture.detectChanges();

      const breakdown = component.payoutBreakdown();
      expect(breakdown).toBeTruthy();
      const sum = breakdown!.house_rake + breakdown!.distributable_pot;
      expect(sum).toBe(breakdown!.total_pot);
    });
  });

  describe('Per-option stake controls', () => {
    const contestWithUserPrediction: Contest = {
      id: 42,
      circle_id: 1,
      creator_id: 1,
      question: 'Who wins?',
      options: [
        { id: 1, text: 'A' },
        { id: 2, text: 'B' },
      ],
      predictions: [
        {
          user_id: 1,
          option_id: 1,
          clout: 500,
          timestamp: '2024-01-01T12:00:00Z',
        },
      ],
      status: 'OPEN',
      min_stake: 100,
      total_pot: 500,
      house_rake: 50,
      created_at: '2024-01-01T00:00:00Z',
      closes_at: '2024-01-02T00:00:00Z',
      duration: '1d',
    };

    beforeEach(() => {
      mockContestService.pollContestDetails.and.returnValue(
        of({ ...contestWithUserPrediction, totals: { byOption: new Map() } }),
      );
      mockContestService.makePrediction.and.returnValue(of(void 0));

      fixture.detectChanges();
    });

    it('prefills stake from existing user prediction', () => {
      expect(component.getStakeForOption(1)).toBe(500);
    });

    it('clamps decrements to the minimum stake', () => {
      component.adjustStake(1, -1000);
      expect(component.getStakeForOption(1)).toBe(100);
    });

    it('sends the updated stake for the chosen option', () => {
      component.setStake(1, 1200);
      component.placePrediction(1);

      expect(mockContestService.makePrediction).toHaveBeenCalledWith(1, 42, {
        option_id: 1,
        clout: 1200,
      });
    });
  });

  describe('Polling Lifecycle', () => {
    it('should start polling on init', () => {
      const contestWithTotals = {
        ...mockOpenContest,
        totals: { byOption: new Map() },
      };
      mockContestService.pollContestDetails.and.returnValue(
        of(contestWithTotals),
      );

      fixture.detectChanges();

      expect(mockContestService.pollContestDetails).toHaveBeenCalledWith(1, 1);
      expect(component.contest()).toEqual(contestWithTotals);
    });

    it('should unsubscribe from polling on destroy', () => {
      type ContestWithTotals = Contest & {
        totals: { byOption: Map<number, { clout: number; count: number }> };
      };
      const pollSubject = new Subject<ContestWithTotals>();
      mockContestService.pollContestDetails.and.returnValue(
        pollSubject.asObservable(),
      );

      fixture.detectChanges();

      const subscription = (component as any).pollingSubscription;
      spyOn(subscription, 'unsubscribe');

      fixture.destroy();

      expect(subscription.unsubscribe).toHaveBeenCalled();
    });

    it('should update contest state on each poll emission', (done) => {
      const contestOpen = {
        ...mockOpenContest,
        totals: { byOption: new Map([[1, { clout: 100, count: 1 }]]) },
      };
      const contestUpdated = {
        ...mockOpenContest,
        totals: { byOption: new Map([[1, { clout: 300, count: 3 }]]) },
      };

      const pollSubject = new Subject<typeof contestOpen>();
      mockContestService.pollContestDetails.and.returnValue(
        pollSubject.asObservable(),
      );

      fixture.detectChanges();

      // First emission
      pollSubject.next(contestOpen);
      expect(component.contest()).toEqual(contestOpen);

      // Second emission
      pollSubject.next(contestUpdated);
      expect(component.contest()).toEqual(contestUpdated);

      pollSubject.complete();
      done();
    });

    it('should load payout breakdown only once when contest becomes resolved', () => {
      const contestLocked = {
        ...mockOpenContest,
        status: 'LOCKED' as const,
        totals: { byOption: new Map() },
      };
      const contestResolved = {
        ...mockResolvedContest,
        totals: { byOption: new Map() },
      };

      const pollSubject = new Subject<any>();
      mockContestService.pollContestDetails.and.returnValue(
        pollSubject.asObservable(),
      );
      mockContestService.getPayoutBreakdown.and.returnValue(
        of(mockPayoutBreakdown),
      );

      fixture.detectChanges();

      // First emission: LOCKED (no payout breakdown)
      pollSubject.next(contestLocked);
      expect(mockContestService.getPayoutBreakdown).not.toHaveBeenCalled();

      // Second emission: RESOLVED (should load payout breakdown)
      pollSubject.next(contestResolved);
      expect(mockContestService.getPayoutBreakdown).toHaveBeenCalledWith(1, 1);
      expect(mockContestService.getPayoutBreakdown).toHaveBeenCalledTimes(1);

      // Third emission: RESOLVED again (should not load payout breakdown again)
      pollSubject.next(contestResolved);
      expect(mockContestService.getPayoutBreakdown).toHaveBeenCalledTimes(1);
    });
  });
});
