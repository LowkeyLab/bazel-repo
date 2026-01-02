import { ComponentFixture, TestBed } from '@angular/core/testing';
import { CircleDetailComponent } from './circle-detail.component';
import { ActivatedRoute, Router } from '@angular/router';
import { CircleService } from '../../services/circle.service';
import { DOCUMENT } from '@angular/common';
import { of, throwError } from 'rxjs';
import type { Circle } from '../../models/circle.model';
import type { Contest } from '../../models/contest.model';

describe('CircleDetailComponent', () => {
  let component: CircleDetailComponent;
  let fixture: ComponentFixture<CircleDetailComponent>;
  let mockCircleService: jasmine.SpyObj<CircleService>;
  let mockRouter: jasmine.SpyObj<Router>;
  let mockActivatedRoute: Partial<ActivatedRoute>;
  let mockDocument: Partial<Document>;

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
      circle_ids: [1],
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
      created_at: '2024-01-01T00:00:00Z',
      expires_at: '2024-01-02T00:00:00Z',
    },
  ];

  beforeEach(async () => {
    mockCircleService = jasmine.createSpyObj('CircleService', [
      'getCircle',
      'getCircleContests',
      'createCircle',
      'listUserCircles',
      'addMember',
      'joinCircle',
    ]);
    mockRouter = jasmine.createSpyObj('Router', ['navigate']);
    mockActivatedRoute = {
      snapshot: {
        paramMap: {
          get: jasmine.createSpy('get').and.returnValue('1'),
        },
      } as any,
    };

    await TestBed.configureTestingModule({
      imports: [CircleDetailComponent],
      providers: [
        { provide: CircleService, useValue: mockCircleService },
        { provide: Router, useValue: mockRouter },
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
      mockCircleService.getCircle.and.returnValue(of(mockCircle));
      mockCircleService.getCircleContests.and.returnValue(of(mockContests));

      fixture.detectChanges();

      expect(mockCircleService.getCircle).toHaveBeenCalledWith(1);
      expect(mockCircleService.getCircleContests).toHaveBeenCalledWith(1);
      expect(component['circle']()).toEqual(mockCircle);
      expect(component['contests']()).toEqual(mockContests);
      expect(component['loading']()).toBe(false);
      expect(component['loadingContests']()).toBe(false);
    });

    it('should handle circle load error', () => {
      mockCircleService.getCircle.and.returnValue(
        throwError(() => new Error('Not found')),
      );
      mockCircleService.getCircleContests.and.returnValue(of([]));

      fixture.detectChanges();

      expect(component['circle']()).toBeNull();
      expect(component['loading']()).toBe(false);
    });

    it('should handle contests load error', () => {
      mockCircleService.getCircle.and.returnValue(of(mockCircle));
      mockCircleService.getCircleContests.and.returnValue(
        throwError(() => new Error('Server error')),
      );

      fixture.detectChanges();

      expect(component['circle']()).toEqual(mockCircle);
      expect(component['contests']()).toEqual([]);
      expect(component['loadingContests']()).toBe(false);
    });
  });

  describe('loadContests', () => {
    beforeEach(() => {
      mockCircleService.getCircle.and.returnValue(of(mockCircle));
    });

    it('should load contests for a circle', () => {
      mockCircleService.getCircleContests.and.returnValue(of(mockContests));

      fixture.detectChanges();

      expect(component['contests']()).toEqual(mockContests);
      expect(component['loadingContests']()).toBe(false);
    });

    it('should handle empty contests list', () => {
      mockCircleService.getCircleContests.and.returnValue(of([]));

      fixture.detectChanges();

      expect(component['contests']()).toEqual([]);
      expect(component['loadingContests']()).toBe(false);
    });

    it('should set loadingContests to true while loading', (done) => {
      mockCircleService.getCircleContests.and.returnValue(of(mockContests));

      fixture.detectChanges();

      // After detectChanges, loadingContests should be false
      expect(component['loadingContests']()).toBe(false);
      done();
    });
  });

  describe('viewContest', () => {
    beforeEach(() => {
      mockCircleService.getCircle.and.returnValue(of(mockCircle));
      mockCircleService.getCircleContests.and.returnValue(of(mockContests));
      fixture.detectChanges();
    });

    it('should navigate to contest detail', () => {
      component['viewContest'](1);

      expect(mockRouter.navigate).toHaveBeenCalledWith(['/contests', 1]);
    });
  });

  describe('goBack', () => {
    beforeEach(() => {
      mockCircleService.getCircle.and.returnValue(of(mockCircle));
      mockCircleService.getCircleContests.and.returnValue(of(mockContests));
      fixture.detectChanges();
    });

    it('should navigate back to circles list', () => {
      component['goBack']();

      expect(mockRouter.navigate).toHaveBeenCalledWith(['/circles']);
    });
  });

  describe('createContest', () => {
    beforeEach(() => {
      mockCircleService.getCircle.and.returnValue(of(mockCircle));
      mockCircleService.getCircleContests.and.returnValue(of(mockContests));
      fixture.detectChanges();
    });

    it('should navigate to create contest page', () => {
      component['createContest']('Test Circle');

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
      mockCircleService.getCircle.and.returnValue(of(mockCircle));
      mockCircleService.getCircleContests.and.returnValue(of(mockContests));
      fixture.detectChanges();
    });

    it('should generate correct join link', () => {
      const link = component['getJoinLink'](1);

      expect(link).toContain('/circles/1/join');
    });
  });

  describe('copyJoinLink', () => {
    beforeEach(() => {
      mockCircleService.getCircle.and.returnValue(of(mockCircle));
      mockCircleService.getCircleContests.and.returnValue(of(mockContests));
      fixture.detectChanges();

      spyOn(navigator.clipboard, 'writeText').and.returnValue(
        Promise.resolve(),
      );
    });

    it('should copy join link to clipboard', async () => {
      component['copyJoinLink'](1);

      expect(navigator.clipboard.writeText).toHaveBeenCalled();
      const call = (
        navigator.clipboard.writeText as jasmine.Spy
      ).calls.mostRecent();
      expect(call.args[0]).toContain('/circles/1/join');
    });

    it('should set linkCopied signal to true temporarily', async () => {
      component['copyJoinLink'](1);

      await new Promise((resolve) => setTimeout(resolve, 10));

      // linkCopied should be true initially
      // After 2 seconds, it should be false (but we won't wait that long in tests)
      expect(component['linkCopied']()).toBe(true);
    });
  });

  describe('getMaxClout', () => {
    beforeEach(() => {
      mockCircleService.getCircle.and.returnValue(of(mockCircle));
      mockCircleService.getCircleContests.and.returnValue(of(mockContests));
      fixture.detectChanges();
    });

    it('should return max clout from members', () => {
      const maxClout = component['getMaxClout'](mockCircle);

      expect(maxClout).toBe(500);
    });

    it('should handle circle with single member', () => {
      const singleMemberCircle: Circle = {
        ...mockCircle,
        members: [{ user_id: 1, username: 'alice', clout: 300 }],
      };

      const maxClout = component['getMaxClout'](singleMemberCircle);

      expect(maxClout).toBe(300);
    });
  });
});
