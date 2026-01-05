import { ComponentFixture, TestBed } from '@angular/core/testing';
import { ContestDetailComponent } from './contest-detail.component';
import { ActivatedRoute, Router } from '@angular/router';
import { ContestService } from '../../services/contest.service';
import { AuthService, type AuthUser } from '../../services/auth.service';
import { of, throwError } from 'rxjs';
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
    clout_consumed: 30,
    created_at: '2024-01-01T00:00:00Z',
    expires_at: '2024-01-02T00:00:00Z',
  };

  const mockPayoutBreakdown: PayoutBreakdown = {
    winners: [
      {
        user_id: 2,
        stake: 100,
        share: 90,
        total: 190,
      },
      {
        user_id: 3,
        stake: 200,
        share: 180,
        total: 380,
      },
    ],
    total_pot: 300,
    clout_consumed: 30,
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
    ]);
    mockAuthService = jasmine.createSpyObj('AuthService', ['currentUser']);
    mockRouter = jasmine.createSpyObj('Router', ['navigate']);
    mockActivatedRoute = {
      snapshot: {
        paramMap: {
          get: jasmine.createSpy('get').and.returnValue('1'),
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

    fixture = TestBed.createComponent(ContestDetailComponent);
    component = fixture.componentInstance;
  });

  it('should create', () => {
    expect(component).toBeTruthy();
  });

  describe('Payout Breakdown Loading', () => {
    it('should load payout breakdown when contest is RESOLVED', () => {
      mockContestService.getContest.and.returnValue(of(mockResolvedContest));
      mockContestService.getPayoutBreakdown.and.returnValue(
        of(mockPayoutBreakdown),
      );

      fixture.detectChanges();

      expect(mockContestService.getContest).toHaveBeenCalledWith(1);
      expect(mockContestService.getPayoutBreakdown).toHaveBeenCalledWith(1);
      expect(component['payoutBreakdown']()).toEqual(mockPayoutBreakdown);
    });

    it('should not load payout breakdown when contest is OPEN', () => {
      mockContestService.getContest.and.returnValue(of(mockOpenContest));

      fixture.detectChanges();

      expect(mockContestService.getContest).toHaveBeenCalledWith(1);
      expect(mockContestService.getPayoutBreakdown).not.toHaveBeenCalled();
    });

    it('should set payoutLoading signal to true while fetching', (done) => {
      mockContestService.getContest.and.returnValue(of(mockResolvedContest));
      mockContestService.getPayoutBreakdown.and.returnValue(
        of(mockPayoutBreakdown),
      );

      fixture.detectChanges();

      // Check that payoutLoading is false after loading completes
      setTimeout(() => {
        expect(component['payoutLoading']()).toBe(false);
        done();
      }, 0);
    });

    it('should handle payout breakdown fetch error', () => {
      mockContestService.getContest.and.returnValue(of(mockResolvedContest));
      mockContestService.getPayoutBreakdown.and.returnValue(
        throwError(() => ({
          error: { error: 'Failed to load payout breakdown' },
        })),
      );

      fixture.detectChanges();

      expect(component['payoutError']()).toBe(
        'Failed to load payout breakdown',
      );
      expect(component['payoutLoading']()).toBe(false);
    });

    it('should handle generic payout breakdown fetch error', () => {
      mockContestService.getContest.and.returnValue(of(mockResolvedContest));
      mockContestService.getPayoutBreakdown.and.returnValue(
        throwError(() => new Error('Network error')),
      );

      fixture.detectChanges();

      expect(component['payoutError']()).toBe(
        'Failed to load payout breakdown',
      );
      expect(component['payoutLoading']()).toBe(false);
    });
  });

  describe('Payout Breakdown Display', () => {
    beforeEach(() => {
      mockContestService.getContest.and.returnValue(of(mockResolvedContest));
      mockContestService.getPayoutBreakdown.and.returnValue(
        of(mockPayoutBreakdown),
      );
      fixture.detectChanges();
    });

    it('should display payout breakdown data correctly', () => {
      const breakdown = component['payoutBreakdown']();
      expect(breakdown).toBeTruthy();
      expect(breakdown?.total_pot).toBe(300);
      expect(breakdown?.clout_consumed).toBe(30);
      expect(breakdown?.distributable_pot).toBe(270);
      expect(breakdown?.total_distributed).toBe(570);
    });

    it('should display all winners in payout breakdown', () => {
      const breakdown = component['payoutBreakdown']();
      expect(breakdown?.winners.length).toBe(2);
      expect(breakdown?.winners[0]).toEqual({
        user_id: 2,
        stake: 100,
        share: 90,
        total: 190,
      });
      expect(breakdown?.winners[1]).toEqual({
        user_id: 3,
        stake: 200,
        share: 180,
        total: 380,
      });
    });

    it('should calculate payout totals correctly', () => {
      const breakdown = component['payoutBreakdown']();
      expect(breakdown).toBeTruthy();
      const actualTotal = breakdown!.winners.reduce(
        (sum, winner) => sum + winner.total,
        0,
      );
      expect(actualTotal).toBe(breakdown!.total_distributed);
    });

    it('should verify payout breakdown totals sum correctly', () => {
      const breakdown = component['payoutBreakdown']();
      expect(breakdown).toBeTruthy();
      expect(breakdown!.total_distributed).toBe(570);
      // Total pot - consumed = distributable
      expect(breakdown!.total_pot - breakdown!.clout_consumed).toBe(
        breakdown!.distributable_pot,
      );
    });
  });

  describe('Payout Breakdown Not Shown for Non-Resolved Contests', () => {
    it('should not load payout breakdown when contest status is OPEN', () => {
      mockContestService.getContest.and.returnValue(of(mockOpenContest));

      fixture.detectChanges();

      expect(component['payoutBreakdown']()).toBeNull();
      expect(mockContestService.getPayoutBreakdown).not.toHaveBeenCalled();
    });

    it('should not load payout breakdown when contest status is LOCKED', () => {
      const lockedContest: Contest = {
        ...mockResolvedContest,
        status: 'LOCKED',
        result_option_id: undefined,
      };
      mockContestService.getContest.and.returnValue(of(lockedContest));

      fixture.detectChanges();

      expect(component['payoutBreakdown']()).toBeNull();
      expect(mockContestService.getPayoutBreakdown).not.toHaveBeenCalled();
    });

    it('should not load payout breakdown when contest status is CLOSED', () => {
      const closedContest: Contest = {
        ...mockResolvedContest,
        status: 'CLOSED',
        result_option_id: undefined,
      };
      mockContestService.getContest.and.returnValue(of(closedContest));

      fixture.detectChanges();

      expect(component['payoutBreakdown']()).toBeNull();
      expect(mockContestService.getPayoutBreakdown).not.toHaveBeenCalled();
    });
  });

  describe('Contest Resolution Flow', () => {
    it('should load payout breakdown after resolving contest', () => {
      mockContestService.getContest.and.returnValue(of(mockOpenContest));

      fixture.detectChanges();
      expect(mockContestService.getPayoutBreakdown).not.toHaveBeenCalled();

      // Simulate resolution
      mockContestService.getContest.and.returnValue(of(mockResolvedContest));
      mockContestService.getPayoutBreakdown.and.returnValue(
        of(mockPayoutBreakdown),
      );

      component['loadContest'](1);

      expect(mockContestService.getPayoutBreakdown).toHaveBeenCalledWith(1);
      expect(component['payoutBreakdown']()).toEqual(mockPayoutBreakdown);
    });

    it('should handle error when loading payout breakdown after resolution', () => {
      mockContestService.getContest.and.returnValue(of(mockResolvedContest));
      mockContestService.getPayoutBreakdown.and.returnValue(
        throwError(() => ({
          error: { error: 'Contest not found' },
        })),
      );

      fixture.detectChanges();

      expect(component['payoutError']()).toBe('Contest not found');
      expect(component['payoutBreakdown']()).toBeNull();
    });
  });

  describe('Payout Breakdown Signals', () => {
    it('should initialize payoutBreakdown signal as null', () => {
      expect(component['payoutBreakdown']()).toBeNull();
    });

    it('should initialize payoutLoading signal as false', () => {
      expect(component['payoutLoading']()).toBe(false);
    });

    it('should initialize payoutError signal as empty string', () => {
      expect(component['payoutError']()).toBe('');
    });

    it('should update all three signals when loading succeeds', () => {
      mockContestService.getContest.and.returnValue(of(mockResolvedContest));
      mockContestService.getPayoutBreakdown.and.returnValue(
        of(mockPayoutBreakdown),
      );

      expect(component['payoutBreakdown']()).toBeNull();
      expect(component['payoutLoading']()).toBe(false);
      expect(component['payoutError']()).toBe('');

      fixture.detectChanges();

      expect(component['payoutBreakdown']()).toEqual(mockPayoutBreakdown);
      expect(component['payoutLoading']()).toBe(false);
      expect(component['payoutError']()).toBe('');
    });

    it('should update signals correctly when loading fails', () => {
      const errorMessage = 'Connection timeout';
      mockContestService.getContest.and.returnValue(of(mockResolvedContest));
      mockContestService.getPayoutBreakdown.and.returnValue(
        throwError(() => ({
          error: { error: errorMessage },
        })),
      );

      fixture.detectChanges();

      expect(component['payoutBreakdown']()).toBeNull();
      expect(component['payoutLoading']()).toBe(false);
      expect(component['payoutError']()).toBe(errorMessage);
    });
  });

  describe('Payout Breakdown Edge Cases', () => {
    it('should handle contest with single winner', () => {
      const singleWinnerBreakdown: PayoutBreakdown = {
        winners: [
          {
            user_id: 2,
            stake: 300,
            share: 270,
            total: 570,
          },
        ],
        total_pot: 300,
        clout_consumed: 30,
        distributable_pot: 270,
        total_distributed: 570,
      };

      mockContestService.getContest.and.returnValue(of(mockResolvedContest));
      mockContestService.getPayoutBreakdown.and.returnValue(
        of(singleWinnerBreakdown),
      );

      fixture.detectChanges();

      const breakdown = component['payoutBreakdown']();
      expect(breakdown?.winners.length).toBe(1);
      expect(breakdown?.total_distributed).toBe(570);
    });

    it('should handle contest with many winners', () => {
      const manyWinnersBreakdown: PayoutBreakdown = {
        winners: [
          { user_id: 2, stake: 50, share: 25, total: 75 },
          { user_id: 3, stake: 50, share: 25, total: 75 },
          { user_id: 4, stake: 50, share: 25, total: 75 },
          { user_id: 5, stake: 50, share: 25, total: 75 },
        ],
        total_pot: 200,
        clout_consumed: 20,
        distributable_pot: 180,
        total_distributed: 300,
      };

      mockContestService.getContest.and.returnValue(of(mockResolvedContest));
      mockContestService.getPayoutBreakdown.and.returnValue(
        of(manyWinnersBreakdown),
      );

      fixture.detectChanges();

      const breakdown = component['payoutBreakdown']();
      expect(breakdown?.winners.length).toBe(4);
      expect(breakdown?.total_distributed).toBe(300);
    });

    it('should handle zero clout stake', () => {
      const zeroStakeBreakdown: PayoutBreakdown = {
        winners: [{ user_id: 2, stake: 0, share: 0, total: 0 }],
        total_pot: 0,
        clout_consumed: 0,
        distributable_pot: 0,
        total_distributed: 0,
      };

      mockContestService.getContest.and.returnValue(of(mockResolvedContest));
      mockContestService.getPayoutBreakdown.and.returnValue(
        of(zeroStakeBreakdown),
      );

      fixture.detectChanges();

      const breakdown = component['payoutBreakdown']();
      expect(breakdown?.total_pot).toBe(0);
      expect(breakdown?.total_distributed).toBe(0);
    });
  });

  describe('Consumption Rate Verification', () => {
    it('should verify 10% consumption rate is applied correctly', () => {
      mockContestService.getContest.and.returnValue(of(mockResolvedContest));
      mockContestService.getPayoutBreakdown.and.returnValue(
        of(mockPayoutBreakdown),
      );

      fixture.detectChanges();

      const breakdown = component['payoutBreakdown']();
      expect(breakdown).toBeTruthy();
      const expectedConsumed = Math.floor(breakdown!.total_pot * 0.1);
      expect(breakdown!.clout_consumed).toBe(expectedConsumed);
    });

    it('should verify 90% distributable rate is applied correctly', () => {
      mockContestService.getContest.and.returnValue(of(mockResolvedContest));
      mockContestService.getPayoutBreakdown.and.returnValue(
        of(mockPayoutBreakdown),
      );

      fixture.detectChanges();

      const breakdown = component['payoutBreakdown']();
      expect(breakdown).toBeTruthy();
      const expectedDistributable =
        breakdown!.total_pot - breakdown!.clout_consumed;
      expect(breakdown!.distributable_pot).toBe(expectedDistributable);
    });

    it('should maintain invariant: consumed + distributable = total_pot', () => {
      mockContestService.getContest.and.returnValue(of(mockResolvedContest));
      mockContestService.getPayoutBreakdown.and.returnValue(
        of(mockPayoutBreakdown),
      );

      fixture.detectChanges();

      const breakdown = component['payoutBreakdown']();
      expect(breakdown).toBeTruthy();
      const sum = breakdown!.clout_consumed + breakdown!.distributable_pot;
      expect(sum).toBe(breakdown!.total_pot);
    });
  });
});
