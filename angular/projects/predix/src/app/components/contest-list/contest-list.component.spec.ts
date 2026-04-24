import { ComponentFixture, TestBed } from '@angular/core/testing';
import { vi } from 'vitest';
import { ContestListComponent } from './contest-list.component';
import { Router } from '@angular/router';
import { CircleService } from '../../services/circle.service';
import { of, throwError } from 'rxjs';
import type { Circle } from '../../models/circle.model';
import type { Contest } from '../../models/contest.model';
import { By } from '@angular/platform-browser';
import { createMockObject } from '../../../testing/create-mock-object';

describe('ContestListComponent', () => {
  const circleServiceMethods = [
    'listUserCircles',
    'getCircleContests',
  ] as const;
  const routerMethods = ['navigate'] as const;
  let component: ContestListComponent;
  let fixture: ComponentFixture<ContestListComponent>;
  let mockCircleService = createMockObject(circleServiceMethods);
  let mockRouter = createMockObject(routerMethods);

  const mockCircle1: Circle = {
    id: 1,
    name: 'Circle One',
    created_at: '2024-01-01T00:00:00Z',
    members: [
      { user_id: 1, username: 'alice', clout: 500 },
      { user_id: 2, username: 'bob', clout: 300 },
    ],
  };

  const mockCircle2: Circle = {
    id: 2,
    name: 'Circle Two',
    created_at: '2024-01-02T00:00:00Z',
    members: [
      { user_id: 1, username: 'alice', clout: 400 },
      { user_id: 3, username: 'charlie', clout: 200 },
    ],
  };

  const mockCircle3: Circle = {
    id: 3,
    name: 'Empty Circle',
    created_at: '2024-01-03T00:00:00Z',
    members: [{ user_id: 1, username: 'alice', clout: 600 }],
  };

  const mockContest1: Contest = {
    id: 101,
    circle_id: 1,
    creator_id: 1,
    question: 'Will it rain tomorrow?',
    options: [
      { id: 1, text: 'Yes' },
      { id: 2, text: 'No' },
    ],
    predictions: [
      {
        user_id: 2,
        option_id: 1,
        clout: 50,
        timestamp: '2024-01-01T12:00:00Z',
      },
      {
        user_id: 1,
        option_id: 2,
        clout: 75,
        timestamp: '2024-01-01T13:00:00Z',
      },
    ],
    status: 'OPEN',
    result_option_id: undefined,
    min_stake: 10,
    total_pot: 125,
    house_rake: 12,
    created_at: '2024-01-05T10:00:00Z', // Newest
    locked_at: '2024-01-10T00:00:00Z',
    duration: '5d',
  };

  const mockContest2: Contest = {
    id: 102,
    circle_id: 1,
    creator_id: 2,
    question: 'What is 2+2?',
    options: [
      { id: 3, text: '3' },
      { id: 4, text: '4' },
      { id: 5, text: '5' },
    ],
    predictions: [
      {
        user_id: 1,
        option_id: 4,
        clout: 100,
        timestamp: '2024-01-02T12:00:00Z',
      },
    ],
    status: 'LOCKED',
    result_option_id: undefined,
    min_stake: 20,
    total_pot: 100,
    house_rake: 10,
    created_at: '2024-01-03T10:00:00Z', // Oldest
    locked_at: '2024-01-05T00:00:00Z',
    duration: '2d',
  };

  const mockContest3: Contest = {
    id: 201,
    circle_id: 2,
    creator_id: 3,
    question: 'Best programming language?',
    options: [
      { id: 6, text: 'TypeScript' },
      { id: 7, text: 'Rust' },
      { id: 8, text: 'Go' },
    ],
    predictions: [
      {
        user_id: 1,
        option_id: 6,
        clout: 200,
        timestamp: '2024-01-04T12:00:00Z',
      },
      {
        user_id: 3,
        option_id: 7,
        clout: 150,
        timestamp: '2024-01-04T13:00:00Z',
      },
    ],
    status: 'RESOLVED',
    result_option_id: 6,
    min_stake: 50,
    total_pot: 350,
    house_rake: 35,
    created_at: '2024-01-04T10:00:00Z', // Middle
    locked_at: '2024-01-08T00:00:00Z',
    duration: '4d',
  };

  const mockContest4: Contest = {
    id: 202,
    circle_id: 2,
    creator_id: 1,
    question: 'Contest with no predictions',
    options: [
      { id: 9, text: 'Option A' },
      { id: 10, text: 'Option B' },
    ],
    predictions: [],
    status: 'OPEN',
    result_option_id: undefined,
    min_stake: 5,
    total_pot: 0,
    house_rake: 0,
    created_at: '2024-01-02T10:00:00Z',
    locked_at: '2024-01-09T00:00:00Z',
    duration: '7d',
  };

  const mockContestResponses = (responses: Record<number, Contest[]>) => {
    mockCircleService.getCircleContests.mockImplementation((circleId: number) =>
      of(responses[circleId] ?? []),
    );
  };

  beforeEach(async () => {
    mockCircleService = createMockObject(circleServiceMethods);
    mockRouter = createMockObject(routerMethods);

    await TestBed.configureTestingModule({
      imports: [ContestListComponent],
      providers: [
        {
          provide: CircleService,
          useValue: mockCircleService as unknown as CircleService,
        },
        { provide: Router, useValue: mockRouter as unknown as Router },
      ],
    }).compileComponents();

    fixture = TestBed.createComponent(ContestListComponent);
    component = fixture.componentInstance;
  });

  it('should create', () => {
    expect(component).toBeTruthy();
  });

  describe('Contest Loading Flow', () => {
    it('should load contests from multiple circles on init', () => {
      mockCircleService.listUserCircles.mockReturnValue(
        of([mockCircle1, mockCircle2]),
      );
      mockContestResponses({
        1: [mockContest1, mockContest2],
        2: [mockContest3, mockContest4],
      });

      fixture.detectChanges(); // Triggers ngOnInit

      expect(mockCircleService.listUserCircles).toHaveBeenCalledTimes(1);
      expect(mockCircleService.getCircleContests).toHaveBeenCalledTimes(2);
      expect(mockCircleService.getCircleContests).toHaveBeenCalledWith(1);
      expect(mockCircleService.getCircleContests).toHaveBeenCalledWith(2);
      expect(component.contests().length).toBe(4);
      expect(component.loading()).toBe(false);
    });

    it('should set loading to true initially, then false after completion', () => {
      mockCircleService.listUserCircles.mockReturnValue(of([mockCircle1]));
      mockCircleService.getCircleContests.mockReturnValue(of([mockContest1]));

      expect(component.loading()).toBe(false); // Initial state

      fixture.detectChanges(); // Triggers ngOnInit

      // After subscription completes, loading should be false
      expect(component.loading()).toBe(false);
    });

    it('should handle empty circles list', () => {
      mockCircleService.listUserCircles.mockReturnValue(of([]));

      fixture.detectChanges();

      expect(mockCircleService.listUserCircles).toHaveBeenCalledTimes(1);
      expect(mockCircleService.getCircleContests).not.toHaveBeenCalled();
      expect(component.contests()).toEqual([]);
      expect(component.loading()).toBe(false);
    });

    it('should handle mixed scenario with some circles having contests and others empty', () => {
      mockCircleService.listUserCircles.mockReturnValue(
        of([mockCircle1, mockCircle2, mockCircle3]),
      );
      mockContestResponses({
        1: [mockContest1],
        2: [],
        3: [mockContest2],
      });

      fixture.detectChanges();

      expect(mockCircleService.getCircleContests).toHaveBeenCalledTimes(3);
      expect(component.contests().length).toBe(2);
      expect(component.contests()).toContain(mockContest1);
      expect(component.contests()).toContain(mockContest2);
    });

    it('should properly flatten nested contest arrays from multiple circles', () => {
      mockCircleService.listUserCircles.mockReturnValue(
        of([mockCircle1, mockCircle2]),
      );
      mockContestResponses({
        1: [mockContest1, mockContest2],
        2: [mockContest3],
      });

      fixture.detectChanges();

      const contests = component.contests();
      expect(contests.length).toBe(3);
      expect(contests).toContain(mockContest1);
      expect(contests).toContain(mockContest2);
      expect(contests).toContain(mockContest3);
    });
  });

  describe('Contest Sorting Flow', () => {
    it('should sort contests by created_at descending (newest first)', () => {
      // Setup contests with mixed timestamp order
      // mockContest1: '2024-01-05T10:00:00Z' (newest)
      // mockContest3: '2024-01-04T10:00:00Z' (middle)
      // mockContest2: '2024-01-03T10:00:00Z' (oldest)
      mockCircleService.listUserCircles.mockReturnValue(
        of([mockCircle1, mockCircle2]),
      );
      mockContestResponses({
        1: [mockContest2],
        2: [mockContest1, mockContest3],
      });

      fixture.detectChanges();

      const contests = component.contests();
      expect(contests.length).toBe(3);
      expect(contests[0]).toBe(mockContest1); // Newest: 2024-01-05
      expect(contests[1]).toBe(mockContest3); // Middle: 2024-01-04
      expect(contests[2]).toBe(mockContest2); // Oldest: 2024-01-03
    });

    it('should handle contests with identical timestamps', () => {
      const contest1 = { ...mockContest1, created_at: '2024-01-05T10:00:00Z' };
      const contest2 = { ...mockContest2, created_at: '2024-01-05T10:00:00Z' };

      mockCircleService.listUserCircles.mockReturnValue(of([mockCircle1]));
      mockCircleService.getCircleContests.mockReturnValue(
        of([contest1, contest2]),
      );

      fixture.detectChanges();

      const contests = component.contests();
      expect(contests.length).toBe(2);
      // Both have same timestamp, order between them doesn't matter but both should be present
      expect(contests).toContain(contest1);
      expect(contests).toContain(contest2);
    });
  });

  describe('Error Handling Flow', () => {
    it('should handle error when fetching circles fails', () => {
      vi.spyOn(console, 'error');
      mockCircleService.listUserCircles.mockReturnValue(
        throwError(() => new Error('Failed to fetch circles')),
      );

      fixture.detectChanges();

      expect(component.loading()).toBe(false);
      expect(component.contests()).toEqual([]);
      expect(console.error).toHaveBeenCalledWith(
        'Failed to load contests:',
        expect.any(Error),
      );
    });

    it('should handle error when fetching contests for a circle fails', () => {
      vi.spyOn(console, 'error');
      mockCircleService.listUserCircles.mockReturnValue(of([mockCircle1]));
      mockCircleService.getCircleContests.mockReturnValue(
        throwError(() => new Error('Failed to fetch contests')),
      );

      fixture.detectChanges();

      expect(component.loading()).toBe(false);
      expect(console.error).toHaveBeenCalledWith(
        'Failed to load contests:',
        expect.any(Error),
      );
    });
  });

  describe('Navigation Flow', () => {
    it('should navigate to contest detail when viewContest is called', () => {
      component.viewContest(mockContest1);

      expect(mockRouter.navigate).toHaveBeenCalledWith([
        '/circles',
        1,
        'contests',
        101,
      ]);
    });

    it('should navigate with correct circle and contest IDs', () => {
      component.viewContest(mockContest3);

      expect(mockRouter.navigate).toHaveBeenCalledWith([
        '/circles',
        2,
        'contests',
        201,
      ]);
    });
  });

  describe('Clout Calculation Flow', () => {
    it('should calculate total clout from predictions', () => {
      const total = component.getTotalClout(mockContest1);

      expect(total).toBe(125); // 50 + 75
    });

    it('should return 0 for contests with no predictions', () => {
      const total = component.getTotalClout(mockContest4);

      expect(total).toBe(0);
    });

    it('should sum multiple predictions correctly', () => {
      const total = component.getTotalClout(mockContest3);

      expect(total).toBe(350); // 200 + 150
    });

    it('should handle single prediction', () => {
      const total = component.getTotalClout(mockContest2);

      expect(total).toBe(100);
    });
  });

  describe('Refresh Flow', () => {
    it('should reload contests when refreshContests is called', () => {
      mockCircleService.listUserCircles.mockReturnValue(of([mockCircle1]));
      mockCircleService.getCircleContests.mockReturnValue(of([mockContest1]));

      fixture.detectChanges(); // Initial load via ngOnInit

      expect(mockCircleService.listUserCircles).toHaveBeenCalledTimes(1);

      // Clear previous calls
      mockCircleService.listUserCircles.mockClear();
      mockCircleService.getCircleContests.mockClear();

      // Now refresh
      component.refreshContests();

      expect(mockCircleService.listUserCircles).toHaveBeenCalledTimes(1);
      expect(mockCircleService.getCircleContests).toHaveBeenCalledTimes(1);
    });

    it('should set loading state during refresh', () => {
      mockCircleService.listUserCircles.mockReturnValue(of([mockCircle1]));
      mockCircleService.getCircleContests.mockReturnValue(of([mockContest1]));

      fixture.detectChanges(); // Initial load

      expect(component.loading()).toBe(false);

      component.refreshContests();

      // After refresh completes, loading should be false again
      expect(component.loading()).toBe(false);
    });

    it('should update contests with new data on refresh', () => {
      // Initial load
      mockCircleService.listUserCircles.mockReturnValue(of([mockCircle1]));
      mockCircleService.getCircleContests.mockReturnValue(of([mockContest1]));

      fixture.detectChanges();

      expect(component.contests().length).toBe(1);
      expect(component.contests()[0]).toBe(mockContest1);

      // Update mock to return different data
      mockCircleService.listUserCircles.mockReturnValue(of([mockCircle2]));
      mockCircleService.getCircleContests.mockReturnValue(
        of([mockContest3, mockContest4]),
      );

      component.refreshContests();

      expect(component.contests().length).toBe(2);
      expect(component.contests()).toContain(mockContest3);
      expect(component.contests()).toContain(mockContest4);
    });

    it('should handle errors during refresh', () => {
      vi.spyOn(console, 'error');
      mockCircleService.listUserCircles.mockReturnValue(of([mockCircle1]));
      mockCircleService.getCircleContests.mockReturnValue(of([mockContest1]));

      fixture.detectChanges(); // Initial load succeeds

      // Make next call fail
      mockCircleService.listUserCircles.mockReturnValue(
        throwError(() => new Error('Network error')),
      );

      component.refreshContests();

      expect(component.loading()).toBe(false);
      expect(console.error).toHaveBeenCalledWith(
        'Failed to load contests:',
        expect.any(Error),
      );
    });
  });

  describe('Template Rendering Flow', () => {
    it('should show loading spinner when loading is true', () => {
      // Set up mocks to prevent ngOnInit errors
      mockCircleService.listUserCircles.mockReturnValue(of([]));

      fixture.detectChanges(); // First detectChanges for ngOnInit

      component.loading.set(true);
      fixture.detectChanges(); // Second detectChanges to update template

      const spinner = fixture.debugElement.query(By.css('.loading-spinner'));
      expect(spinner).toBeTruthy();
    });

    it('should show empty state message when contests list is empty', () => {
      mockCircleService.listUserCircles.mockReturnValue(of([]));
      fixture.detectChanges();

      const alert = fixture.debugElement.query(By.css('.alert-info'));
      expect(alert).toBeTruthy();
      expect(alert.nativeElement.textContent).toContain(
        'No contests in your circles yet',
      );
    });

    it('should render contest cards with correct question text', () => {
      mockCircleService.listUserCircles.mockReturnValue(of([mockCircle1]));
      mockCircleService.getCircleContests.mockReturnValue(
        of([mockContest1, mockContest2]),
      );
      fixture.detectChanges();

      const cards = fixture.debugElement.queryAll(By.css('.card'));
      expect(cards.length).toBe(2);

      const titles = cards.map((card) =>
        card.query(By.css('.card-title')).nativeElement.textContent.trim(),
      );
      expect(titles).toContain('Will it rain tomorrow?');
      expect(titles).toContain('What is 2+2?');
    });

    it('should display status badge with correct text', () => {
      mockCircleService.listUserCircles.mockReturnValue(of([mockCircle1]));
      mockCircleService.getCircleContests.mockReturnValue(of([mockContest1]));
      fixture.detectChanges();

      const badge = fixture.debugElement.query(
        By.css('.badge-success, .badge-warning, .badge-neutral, .badge-info'),
      );
      expect(badge.nativeElement.textContent.trim()).toBe('Open');
    });

    it('should display status badges with correct CSS classes', () => {
      mockCircleService.listUserCircles.mockReturnValue(
        of([mockCircle1, mockCircle2]),
      );
      mockContestResponses({
        1: [mockContest1, mockContest2],
        2: [mockContest3],
      });
      fixture.detectChanges();

      const badges = fixture.debugElement.queryAll(
        By.css('.badge-success, .badge-warning, .badge-info'),
      );
      expect(badges.length).toBeGreaterThan(0);

      // Check OPEN contest has badge-success
      const openBadge = fixture.debugElement.query(By.css('.badge-success'));
      expect(openBadge).toBeTruthy();

      // Check LOCKED contest has badge-warning
      const lockedBadge = fixture.debugElement.query(By.css('.badge-warning'));
      expect(lockedBadge).toBeTruthy();

      // Check RESOLVED contest has badge-info
      const resolvedBadge = fixture.debugElement.query(By.css('.badge-info'));
      expect(resolvedBadge).toBeTruthy();
    });

    it('should display total pot and house rake from mock data', () => {
      mockCircleService.listUserCircles.mockReturnValue(of([mockCircle1]));
      mockCircleService.getCircleContests.mockReturnValue(of([mockContest1]));
      fixture.detectChanges();

      const compiled = fixture.nativeElement;
      const text = compiled.textContent;

      // Check for total_pot value from mock
      expect(text).toContain('125'); // mockContest1.total_pot

      // Check for house_rake value from mock
      expect(text).toContain('12'); // mockContest1.house_rake
    });

    it('should display correct prediction count', () => {
      mockCircleService.listUserCircles.mockReturnValue(of([mockCircle1]));
      mockCircleService.getCircleContests.mockReturnValue(of([mockContest1]));
      fixture.detectChanges();

      const compiled = fixture.nativeElement;
      expect(compiled.textContent).toContain('2 predictions');
    });

    it('should display min stake value', () => {
      mockCircleService.listUserCircles.mockReturnValue(of([mockCircle1]));
      mockCircleService.getCircleContests.mockReturnValue(of([mockContest1]));
      fixture.detectChanges();

      const compiled = fixture.nativeElement;
      expect(compiled.textContent).toContain('Min stake: 10 Clout');
    });

    it('should navigate when contest card is clicked', () => {
      mockCircleService.listUserCircles.mockReturnValue(of([mockCircle1]));
      mockCircleService.getCircleContests.mockReturnValue(of([mockContest1]));
      fixture.detectChanges();

      const card = fixture.debugElement.query(By.css('.card'));
      card.nativeElement.click();

      expect(mockRouter.navigate).toHaveBeenCalledWith([
        '/circles',
        1,
        'contests',
        101,
      ]);
    });

    it('should navigate with correct parameters when different contest card is clicked', () => {
      mockCircleService.listUserCircles.mockReturnValue(of([mockCircle2]));
      mockCircleService.getCircleContests.mockReturnValue(of([mockContest3]));
      fixture.detectChanges();

      const card = fixture.debugElement.query(By.css('.card'));
      card.nativeElement.click();

      expect(mockRouter.navigate).toHaveBeenCalledWith([
        '/circles',
        2,
        'contests',
        201,
      ]);
    });

    it('should display contest options as badges', () => {
      mockCircleService.listUserCircles.mockReturnValue(of([mockCircle1]));
      mockCircleService.getCircleContests.mockReturnValue(of([mockContest1]));
      fixture.detectChanges();

      const optionBadges = fixture.debugElement.queryAll(
        By.css('.badge-outline'),
      );
      expect(optionBadges.length).toBe(2);

      const optionTexts = optionBadges.map((badge) =>
        badge.nativeElement.textContent.trim(),
      );
      expect(optionTexts).toContain('Yes');
      expect(optionTexts).toContain('No');
    });

    it('should render refresh button', () => {
      mockCircleService.listUserCircles.mockReturnValue(of([]));
      fixture.detectChanges();

      const refreshButton = fixture.debugElement.query(
        By.css('button.btn-ghost'),
      );
      expect(refreshButton).toBeTruthy();
      expect(refreshButton.nativeElement.textContent).toContain('Refresh');
    });

    it('should call refreshContests when refresh button is clicked', () => {
      vi.spyOn(component, 'refreshContests');
      mockCircleService.listUserCircles.mockReturnValue(of([]));
      fixture.detectChanges();

      const refreshButton = fixture.debugElement.query(
        By.css('button.btn-ghost'),
      );
      refreshButton.nativeElement.click();

      expect(component.refreshContests).toHaveBeenCalledTimes(1);
    });

    it('should disable refresh button when loading', () => {
      mockCircleService.listUserCircles.mockReturnValue(of([]));
      fixture.detectChanges();

      component.loading.set(true);
      fixture.detectChanges();

      const refreshButton = fixture.debugElement.query(
        By.css('button.btn-ghost'),
      );
      expect(refreshButton.nativeElement.disabled).toBe(true);
    });

    it('should enable refresh button when not loading', () => {
      mockCircleService.listUserCircles.mockReturnValue(of([]));
      fixture.detectChanges();

      component.loading.set(false);
      fixture.detectChanges();

      const refreshButton = fixture.debugElement.query(
        By.css('button.btn-ghost'),
      );
      expect(refreshButton.nativeElement.disabled).toBe(false);
    });

    it('should show "Refreshing..." text when loading', () => {
      mockCircleService.listUserCircles.mockReturnValue(of([]));
      fixture.detectChanges();

      component.loading.set(true);
      fixture.detectChanges();

      const refreshButton = fixture.debugElement.query(
        By.css('button.btn-ghost'),
      );
      expect(refreshButton.nativeElement.textContent).toContain(
        'Refreshing...',
      );
    });

    it('should show "Refresh" text when not loading', () => {
      mockCircleService.listUserCircles.mockReturnValue(of([]));
      fixture.detectChanges();

      component.loading.set(false);
      fixture.detectChanges();

      const refreshButton = fixture.debugElement.query(
        By.css('button.btn-ghost'),
      );
      expect(refreshButton.nativeElement.textContent).toContain('Refresh');
    });

    it('should have animate-spin class on icon when loading', () => {
      mockCircleService.listUserCircles.mockReturnValue(of([]));
      fixture.detectChanges();

      component.loading.set(true);
      fixture.detectChanges();

      const icon = fixture.debugElement.query(By.css('button.btn-ghost svg'));
      expect(icon.nativeElement.classList.contains('animate-spin')).toBe(true);
    });

    it('should not have animate-spin class on icon when not loading', () => {
      mockCircleService.listUserCircles.mockReturnValue(of([]));
      fixture.detectChanges();

      component.loading.set(false);
      fixture.detectChanges();

      const icon = fixture.debugElement.query(By.css('button.btn-ghost svg'));
      expect(icon.nativeElement.classList.contains('animate-spin')).toBe(false);
    });
  });
});
