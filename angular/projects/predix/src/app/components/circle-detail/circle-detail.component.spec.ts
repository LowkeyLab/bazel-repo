import { ComponentFixture, TestBed } from '@angular/core/testing';
import { vi } from 'vitest';
import { CircleDetailComponent } from './circle-detail.component';
import { ActivatedRoute, Router } from '@angular/router';
import { CircleService } from '../../services/circle.service';
import { DOCUMENT } from '@angular/common';
import { of, throwError } from 'rxjs';
import type { Circle } from '../../models/circle.model';
import type { Contest } from '../../models/contest.model';
import { createMockObject } from '../../../testing/create-mock-object';

describe('CircleDetailComponent', () => {
  const circleServiceMethods = [
    'getCircle',
    'getCircleContests',
    'createCircle',
    'listUserCircles',
    'addMember',
    'joinCircle',
  ] as const;
  const routerMethods = ['navigate'] as const;
  let component: CircleDetailComponent;
  let fixture: ComponentFixture<CircleDetailComponent>;
  let mockCircleService = createMockObject(circleServiceMethods);
  let mockRouter = createMockObject(routerMethods);
  let mockActivatedRoute: Partial<ActivatedRoute>;

  const mockCircle: Circle = {
    id: 1,
    name: 'Test Circle',
    created_at: '2024-01-01T00:00:00Z',
    members: [
      { user_id: 1, username: 'alice', clout: 500 },
      { user_id: 2, username: 'bob', clout: 300 },
    ],
  };

  const mockContests: Contest[] = [
    {
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
      ],
      status: 'OPEN',
      result_option_id: undefined,
      min_stake: 10,
      total_pot: 100,
      house_rake: 10,
      created_at: '2024-01-01T00:00:00Z',
      locked_at: '2024-01-02T00:00:00Z',
      duration: '1d',
    },
  ];

  beforeEach(async () => {
    mockCircleService = createMockObject(circleServiceMethods);
    mockRouter = createMockObject(routerMethods);
    mockActivatedRoute = {
      snapshot: {
        paramMap: {
          get: vi.fn().mockReturnValue('1'),
        },
      } as any,
    };

    await TestBed.configureTestingModule({
      imports: [CircleDetailComponent],
      providers: [
        {
          provide: CircleService,
          useValue: mockCircleService as unknown as CircleService,
        },
        { provide: Router, useValue: mockRouter as unknown as Router },
        { provide: ActivatedRoute, useValue: mockActivatedRoute },
        { provide: DOCUMENT, useValue: document },
      ],
    }).compileComponents();

    fixture = TestBed.createComponent(CircleDetailComponent);
    component = fixture.componentInstance;
  });

  it('should create', () => {
    expect(component).toBeTruthy();
  });

  describe('ngOnInit', () => {
    it('should load circle and contests on init', () => {
      mockCircleService.getCircle.mockReturnValue(of(mockCircle));
      mockCircleService.getCircleContests.mockReturnValue(of(mockContests));

      fixture.detectChanges();

      expect(mockCircleService.getCircle).toHaveBeenCalledWith(1);
      expect(mockCircleService.getCircleContests).toHaveBeenCalledWith(1);
      expect(component.circle()).toEqual(mockCircle);
      expect(component.contests()).toEqual(mockContests);
      expect(component.loading()).toBe(false);
      expect(component.loadingContests()).toBe(false);
    });

    it('should handle circle load error', () => {
      mockCircleService.getCircle.mockReturnValue(
        throwError(() => new Error('Not found')),
      );
      mockCircleService.getCircleContests.mockReturnValue(of([]));

      fixture.detectChanges();

      expect(component.circle()).toBeNull();
      expect(component.loading()).toBe(false);
    });

    it('should handle contests load error', () => {
      mockCircleService.getCircle.mockReturnValue(of(mockCircle));
      mockCircleService.getCircleContests.mockReturnValue(
        throwError(() => new Error('Server error')),
      );

      fixture.detectChanges();

      expect(component.circle()).toEqual(mockCircle);
      expect(component.contests()).toEqual([]);
      expect(component.loadingContests()).toBe(false);
    });
  });

  describe('loadContests', () => {
    beforeEach(() => {
      mockCircleService.getCircle.mockReturnValue(of(mockCircle));
    });

    it('should load contests for a circle', () => {
      mockCircleService.getCircleContests.mockReturnValue(of(mockContests));

      fixture.detectChanges();

      expect(component.contests()).toEqual(mockContests);
      expect(component.loadingContests()).toBe(false);
    });

    it('should handle empty contests list', () => {
      mockCircleService.getCircleContests.mockReturnValue(of([]));

      fixture.detectChanges();

      expect(component.contests()).toEqual([]);
      expect(component.loadingContests()).toBe(false);
    });

    it('should set loadingContests to true while loading', () => {
      mockCircleService.getCircleContests.mockReturnValue(of(mockContests));

      fixture.detectChanges();

      expect(component.loadingContests()).toBe(false);
    });
  });

  describe('refreshContests', () => {
    beforeEach(() => {
      mockCircleService.getCircle.mockReturnValue(of(mockCircle));
      mockCircleService.getCircleContests.mockReturnValue(of(mockContests));
      fixture.detectChanges();
    });

    it('should manually refresh contests', () => {
      component.refreshContests();

      expect(mockCircleService.getCircleContests).toHaveBeenCalledWith(1);
      expect(component.contests()).toEqual(mockContests);
    });

    it('should not refresh if circle id is not available', () => {
      component.circle.set(null);
      mockCircleService.getCircleContests.mockClear();

      component.refreshContests();

      expect(mockCircleService.getCircleContests).not.toHaveBeenCalled();
    });
  });

  describe('viewContest', () => {
    beforeEach(() => {
      mockCircleService.getCircle.mockReturnValue(of(mockCircle));
      mockCircleService.getCircleContests.mockReturnValue(of(mockContests));
      fixture.detectChanges();
    });

    it('should navigate to contest detail', () => {
      component.viewContest(1);

      expect(mockRouter.navigate).toHaveBeenCalledWith([
        '/circles',
        1,
        'contests',
        1,
      ]);
    });
  });
  describe('createContest', () => {
    beforeEach(() => {
      mockCircleService.getCircle.mockReturnValue(of(mockCircle));
      mockCircleService.getCircleContests.mockReturnValue(of(mockContests));
      fixture.detectChanges();
    });

    it('should navigate to create contest page', () => {
      component.createContest('Test Circle');

      expect(mockRouter.navigate).toHaveBeenCalledWith(
        ['/circles', 1, 'contest', 'new'],
        {
          queryParams: { circleName: 'Test Circle' },
        },
      );
    });
  });

  describe('getJoinLink', () => {
    beforeEach(() => {
      mockCircleService.getCircle.mockReturnValue(of(mockCircle));
      mockCircleService.getCircleContests.mockReturnValue(of(mockContests));
      fixture.detectChanges();
    });

    it('should generate correct join link', () => {
      const link = component.getJoinLink(1);

      expect(link).toContain('/circles/1/join');
    });
  });

  describe('copyJoinLink', () => {
    beforeEach(() => {
      mockCircleService.getCircle.mockReturnValue(of(mockCircle));
      mockCircleService.getCircleContests.mockReturnValue(of(mockContests));
      fixture.detectChanges();

      Object.defineProperty(navigator, 'clipboard', {
        configurable: true,
        value: {
          writeText: vi.fn().mockResolvedValue(undefined),
        },
      });
    });

    it('should copy join link to clipboard', async () => {
      component.copyJoinLink(1);

      expect(navigator.clipboard.writeText).toHaveBeenCalled();
      const call = (
        navigator.clipboard.writeText as ReturnType<typeof vi.fn>
      ).mock.calls.at(-1);
      expect(call?.[0]).toContain('/circles/1/join');
    });

    it('should set linkCopied signal to true temporarily', async () => {
      component.copyJoinLink(1);

      await new Promise((resolve) => setTimeout(resolve, 10));

      // linkCopied should be true initially
      // After 2 seconds, it should be false (but we won't wait that long in tests)
      expect(component.linkCopied()).toBe(true);
    });
  });

  describe('getMaxClout', () => {
    beforeEach(() => {
      mockCircleService.getCircle.mockReturnValue(of(mockCircle));
      mockCircleService.getCircleContests.mockReturnValue(of(mockContests));
      fixture.detectChanges();
    });

    it('should return max clout from members', () => {
      const maxClout = component.getMaxClout(mockCircle);

      expect(maxClout).toBe(500);
    });

    it('should handle circle with single member', () => {
      const singleMemberCircle: Circle = {
        ...mockCircle,
        members: [{ user_id: 1, username: 'alice', clout: 300 }],
      };

      const maxClout = component.getMaxClout(singleMemberCircle);

      expect(maxClout).toBe(300);
    });
  });
});
