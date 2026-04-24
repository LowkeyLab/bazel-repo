import { ComponentFixture, TestBed } from '@angular/core/testing';
import { vi } from 'vitest';
import { By } from '@angular/platform-browser';
import { Router } from '@angular/router';
import { of, throwError } from 'rxjs';

import { CircleListComponent } from './circle-list.component';
import { CircleService } from '../../services/circle.service';
import type { Circle } from '../../models/circle.model';
import { createMockObject } from '../../../testing/create-mock-object';

describe('CircleListComponent', () => {
  const circleServiceMethods = ['listUserCircles'] as const;
  const routerMethods = ['navigate'] as const;
  let fixture: ComponentFixture<CircleListComponent>;
  let component: CircleListComponent;
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

  beforeEach(async () => {
    mockCircleService = createMockObject(circleServiceMethods);
    mockRouter = createMockObject(routerMethods);

    await TestBed.configureTestingModule({
      imports: [CircleListComponent],
      providers: [
        {
          provide: CircleService,
          useValue: mockCircleService as unknown as CircleService,
        },
        { provide: Router, useValue: mockRouter as unknown as Router },
      ],
    }).compileComponents();

    fixture = TestBed.createComponent(CircleListComponent);
    component = fixture.componentInstance;
  });

  it('should create', () => {
    expect(component).toBeTruthy();
  });

  describe('Initial Load Flow', () => {
    it('should load circles on init', () => {
      mockCircleService.listUserCircles.mockReturnValue(
        of([mockCircle1, mockCircle2]),
      );

      fixture.detectChanges(); // triggers ngOnInit

      expect(mockCircleService.listUserCircles).toHaveBeenCalledTimes(1);
      expect(component.circles().length).toBe(2);
      expect(component.loading()).toBe(false);
    });

    it('should handle error on init', () => {
      vi.spyOn(console, 'error');
      mockCircleService.listUserCircles.mockReturnValue(
        throwError(() => new Error('Failed to fetch circles')),
      );

      fixture.detectChanges();

      expect(component.loading()).toBe(false);
      expect(component.circles()).toEqual([]);
      expect(console.error).toHaveBeenCalledWith(
        'Failed to load circles:',
        expect.any(Error),
      );
    });
  });

  describe('Refresh Flow', () => {
    it('should call service and update circles on refresh', () => {
      // initial
      mockCircleService.listUserCircles.mockReturnValue(of([mockCircle1]));
      fixture.detectChanges();
      expect(component.circles().length).toBe(1);

      // refresh with new data
      mockCircleService.listUserCircles.mockReturnValue(of([mockCircle2]));
      component.refreshCircles();

      expect(mockCircleService.listUserCircles).toHaveBeenCalledTimes(2);
      expect(component.circles().length).toBe(1);
      expect(component.circles()[0]).toBe(mockCircle2);
      expect(component.loading()).toBe(false);
    });

    it('should set loading state during refresh and reset after', () => {
      mockCircleService.listUserCircles.mockReturnValue(of([mockCircle1]));
      fixture.detectChanges();
      expect(component.loading()).toBe(false);

      // Set up next call
      mockCircleService.listUserCircles.mockReturnValue(of([mockCircle2]));
      component.refreshCircles();

      // After synchronous subscription, loading should be false again
      expect(component.loading()).toBe(false);
    });

    it('should handle errors during refresh', () => {
      mockCircleService.listUserCircles.mockReturnValue(of([mockCircle1]));
      fixture.detectChanges();

      vi.spyOn(console, 'error');
      mockCircleService.listUserCircles.mockReturnValue(
        throwError(() => new Error('Network error')),
      );

      component.refreshCircles();

      expect(component.loading()).toBe(false);
      expect(console.error).toHaveBeenCalledWith(
        'Failed to load circles:',
        expect.any(Error),
      );
    });
  });

  describe('Template Rendering Flow', () => {
    it('should render refresh button', () => {
      mockCircleService.listUserCircles.mockReturnValue(of([]));
      fixture.detectChanges();

      const refreshButton = fixture.debugElement.query(
        By.css('button.btn-ghost'),
      );
      expect(refreshButton).toBeTruthy();
      expect(refreshButton.nativeElement.textContent).toContain('Refresh');
    });

    it('should call refreshCircles when refresh button is clicked', () => {
      vi.spyOn(component, 'refreshCircles');
      mockCircleService.listUserCircles.mockReturnValue(of([]));
      fixture.detectChanges();

      const refreshButton = fixture.debugElement.query(
        By.css('button.btn-ghost'),
      );
      refreshButton.nativeElement.click();

      expect(component.refreshCircles).toHaveBeenCalledTimes(1);
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

    it('should show "Refreshing..." text when loading and "Refresh" when not', () => {
      mockCircleService.listUserCircles.mockReturnValue(of([]));
      fixture.detectChanges();

      component.loading.set(true);
      fixture.detectChanges();
      let refreshButton = fixture.debugElement.query(
        By.css('button.btn-ghost'),
      );
      expect(refreshButton.nativeElement.textContent).toContain(
        'Refreshing...',
      );

      component.loading.set(false);
      fixture.detectChanges();
      refreshButton = fixture.debugElement.query(By.css('button.btn-ghost'));
      expect(refreshButton.nativeElement.textContent).toContain('Refresh');
    });

    it('should toggle animate-spin class on icon based on loading', () => {
      mockCircleService.listUserCircles.mockReturnValue(of([]));
      fixture.detectChanges();

      component.loading.set(true);
      fixture.detectChanges();
      let icon = fixture.debugElement.query(By.css('button svg'));
      expect(icon.nativeElement.classList.contains('animate-spin')).toBe(true);

      component.loading.set(false);
      fixture.detectChanges();
      icon = fixture.debugElement.query(By.css('button svg'));
      expect(icon.nativeElement.classList.contains('animate-spin')).toBe(false);
    });
  });
});
