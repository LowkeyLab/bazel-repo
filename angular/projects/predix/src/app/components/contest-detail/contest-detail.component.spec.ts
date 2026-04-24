import { ComponentFixture, TestBed } from '@angular/core/testing';
import { vi } from 'vitest';
import { ContestDetailComponent } from './contest-detail.component';
import { ActivatedRoute, Router } from '@angular/router';
import { ContestService } from '../../services/contest.service';
import { AuthService } from '../../services/auth.service';
import { of, throwError, Subject } from 'rxjs';
import type { Contest, PayoutBreakdown } from '../../models/contest.model';
import { createMockObject } from '../../../testing/create-mock-object';

describe('ContestDetailComponent - Payout Breakdown', () => {
  const contestServiceMethods = [
    'getContest',
    'makePrediction',
    'lockContest',
    'resolveContest',
    'getPayoutBreakdown',
    'streamContestDetails',
  ] as const;
  const authMethods = ['currentUser'] as const;
  const routerMethods = ['navigate'] as const;
  let component: ContestDetailComponent;
  let fixture: ComponentFixture<ContestDetailComponent>;
  let mockContestService = createMockObject(contestServiceMethods);
  let mockAuthService = createMockObject(authMethods);
  let mockRouter = createMockObject(routerMethods);
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
    locked_at: '2024-01-02T00:00:00Z',
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
    mockContestService = createMockObject(contestServiceMethods);
    mockAuthService = createMockObject(authMethods);
    mockRouter = createMockObject(routerMethods);
    mockActivatedRoute = {
      snapshot: {
        paramMap: {
          get: vi.fn().mockImplementation((key: string) => {
            if (key === 'id') return '1';
            if (key === 'circleId') return '1';
            return null;
          }),
        },
      } as any,
    };

    mockAuthService.currentUser.mockReturnValue({
      id: 1,
      username: 'alice',
      role: 'member',
    });

    await TestBed.configureTestingModule({
      imports: [ContestDetailComponent],
      providers: [
        {
          provide: ContestService,
          useValue: mockContestService as unknown as ContestService,
        },
        {
          provide: AuthService,
          useValue: mockAuthService as unknown as AuthService,
        },
        { provide: Router, useValue: mockRouter as unknown as Router },
        { provide: ActivatedRoute, useValue: mockActivatedRoute },
      ],
    }).compileComponents();

    // Default mock for streamContestDetails (can be overridden in specific tests)
    mockContestService.streamContestDetails.mockReturnValue(
      of({
        ...mockOpenContest,
        totals: { byOption: new Map() },
      }),
    );
    mockContestService.getContest.mockReturnValue(of(mockOpenContest));

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
      mockContestService.streamContestDetails.mockReturnValue(
        of(contestWithTotals),
      );
      mockContestService.getPayoutBreakdown.mockReturnValue(
        of(mockPayoutBreakdown),
      );

      fixture.detectChanges();

      expect(mockContestService.streamContestDetails).toHaveBeenCalledWith(
        1,
        1,
      );
      expect(mockContestService.getPayoutBreakdown).toHaveBeenCalledWith(1, 1);
      expect(component.payoutBreakdown()).toBeTruthy();
    });

    it('should not load payout breakdown when contest is OPEN', () => {
      const contestWithTotals = {
        ...mockOpenContest,
        totals: { byOption: new Map() },
      };
      mockContestService.streamContestDetails.mockReturnValue(
        of(contestWithTotals),
      );

      fixture.detectChanges();

      expect(mockContestService.streamContestDetails).toHaveBeenCalledWith(
        1,
        1,
      );
      expect(mockContestService.getPayoutBreakdown).not.toHaveBeenCalled();
    });

    it('should set payoutLoading signal to true while fetching', async () => {
      const contestWithTotals = {
        ...mockResolvedContest,
        totals: { byOption: new Map() },
      };
      mockContestService.streamContestDetails.mockReturnValue(
        of(contestWithTotals),
      );
      mockContestService.getPayoutBreakdown.mockReturnValue(
        of(mockPayoutBreakdown),
      );

      fixture.detectChanges();

      await new Promise((resolve) => setTimeout(resolve, 0));
      expect(component.payoutLoading()).toBe(false);
    });

    it('should handle payout breakdown fetch error', () => {
      mockContestService.streamContestDetails.mockReturnValue(
        of({ ...mockResolvedContest, totals: { byOption: new Map() } }),
      );
      mockContestService.getPayoutBreakdown.mockReturnValue(
        throwError(() => ({
          error: { error: 'Failed to load payout breakdown' },
        })),
      );

      fixture.detectChanges();

      expect(component.payoutError()).toBe('Failed to load payout breakdown');
      expect(component.payoutLoading()).toBe(false);
    });

    it('should handle generic payout breakdown fetch error', () => {
      mockContestService.streamContestDetails.mockReturnValue(
        of({ ...mockResolvedContest, totals: { byOption: new Map() } }),
      );
      mockContestService.getPayoutBreakdown.mockReturnValue(
        throwError(() => new Error('Network error')),
      );

      fixture.detectChanges();

      expect(component.payoutError()).toBe('Failed to load payout breakdown');
      expect(component.payoutLoading()).toBe(false);
    });
  });

  describe('Payout Breakdown Not Shown for Non-Resolved Contests', () => {
    it('should not load payout breakdown when contest status is OPEN', () => {
      mockContestService.streamContestDetails.mockReturnValue(
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
      mockContestService.streamContestDetails.mockReturnValue(
        of({ ...lockedContest, totals: { byOption: new Map() } }),
      );

      fixture.detectChanges();

      expect(component.payoutBreakdown()).toBeNull();
      expect(mockContestService.getPayoutBreakdown).not.toHaveBeenCalled();
    });

    it('should not load payout breakdown when contest status is EXPIRED', () => {
      const closedContest: Contest = {
        ...mockResolvedContest,
        status: 'EXPIRED',
        result_option_id: undefined,
      };
      mockContestService.streamContestDetails.mockReturnValue(
        of({ ...closedContest, totals: { byOption: new Map() } }),
      );

      fixture.detectChanges();

      expect(component.payoutBreakdown()).toBeNull();
      expect(mockContestService.getPayoutBreakdown).not.toHaveBeenCalled();
    });
  });

  describe('Contest Resolution Flow', () => {
    it('should load payout breakdown after resolving contest', () => {
      mockContestService.streamContestDetails.mockReturnValue(
        of({ ...mockOpenContest, totals: { byOption: new Map() } }),
      );

      fixture.detectChanges();
      expect(mockContestService.getPayoutBreakdown).not.toHaveBeenCalled();

      // Simulate resolution
      mockContestService.streamContestDetails.mockReturnValue(
        of({ ...mockResolvedContest, totals: { byOption: new Map() } }),
      );
      mockContestService.getContest.mockReturnValue(of(mockResolvedContest)); // Add getContest mock for loadContest method
      mockContestService.getPayoutBreakdown.mockReturnValue(
        of(mockPayoutBreakdown),
      );

      component.loadContest(1, 1);

      expect(mockContestService.getPayoutBreakdown).toHaveBeenCalledWith(1, 1);
      expect(component.payoutBreakdown()).toBeTruthy();
    });

    it('should handle error when loading payout breakdown after resolution', () => {
      mockContestService.streamContestDetails.mockReturnValue(
        of({ ...mockResolvedContest, totals: { byOption: new Map() } }),
      );
      mockContestService.getPayoutBreakdown.mockReturnValue(
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
      mockContestService.streamContestDetails.mockReturnValue(
        of({ ...mockResolvedContest, totals: { byOption: new Map() } }),
      );
      mockContestService.getPayoutBreakdown.mockReturnValue(
        of(mockPayoutBreakdown),
      );

      expect(component.payoutBreakdown()).toBeNull();
      expect(component.payoutLoading()).toBe(false);
      expect(component.payoutError()).toBe('');

      fixture.detectChanges();

      expect(component.payoutBreakdown()).toBeTruthy();
      expect(component.payoutLoading()).toBe(false);
      expect(component.payoutError()).toBe('');
    });

    it('should update signals correctly when loading fails', () => {
      const errorMessage = 'Connection timeout';
      mockContestService.streamContestDetails.mockReturnValue(
        of({ ...mockResolvedContest, totals: { byOption: new Map() } }),
      );
      mockContestService.getPayoutBreakdown.mockReturnValue(
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

  describe('Polling Lifecycle', () => {
    it('should start polling on init', () => {
      const contestWithTotals = {
        ...mockOpenContest,
        totals: { byOption: new Map() },
      };
      mockContestService.streamContestDetails.mockReturnValue(
        of(contestWithTotals),
      );

      fixture.detectChanges();

      expect(mockContestService.streamContestDetails).toHaveBeenCalledWith(
        1,
        1,
      );
      expect(component.contest()).toEqual(contestWithTotals);
    });

    it('should unsubscribe from polling on destroy', () => {
      type ContestWithTotals = Contest & {
        totals: { byOption: Map<number, { clout: number; count: number }> };
      };
      const pollSubject = new Subject<ContestWithTotals>();
      mockContestService.streamContestDetails.mockReturnValue(
        pollSubject.asObservable(),
      );

      fixture.detectChanges();

      const subscription = (component as any).pollingSubscription;
      vi.spyOn(subscription, 'unsubscribe');

      fixture.destroy();

      expect(subscription.unsubscribe).toHaveBeenCalled();
    });

    it('should update contest state on each poll emission', () => {
      const contestOpen = {
        ...mockOpenContest,
        totals: { byOption: new Map([[1, { clout: 100, count: 1 }]]) },
      };
      const contestUpdated = {
        ...mockOpenContest,
        totals: { byOption: new Map([[1, { clout: 300, count: 3 }]]) },
      };

      const pollSubject = new Subject<typeof contestOpen>();
      mockContestService.streamContestDetails.mockReturnValue(
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
      mockContestService.streamContestDetails.mockReturnValue(
        pollSubject.asObservable(),
      );
      mockContestService.getPayoutBreakdown.mockReturnValue(
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
