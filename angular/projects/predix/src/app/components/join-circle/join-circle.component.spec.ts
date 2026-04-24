import { ComponentFixture, TestBed } from '@angular/core/testing';
import { vi } from 'vitest';
import { JoinCircleComponent } from './join-circle.component';
import { ActivatedRoute, Router } from '@angular/router';
import { CircleService } from '../../services/circle.service';
import { of, throwError } from 'rxjs';
import { createMockObject } from '../../../testing/create-mock-object';

describe('JoinCircleComponent', () => {
  const circleServiceMethods = ['joinCircle'] as const;
  const routerMethods = ['navigate'] as const;
  let component: JoinCircleComponent;
  let fixture: ComponentFixture<JoinCircleComponent>;
  let mockCircleService = createMockObject(circleServiceMethods);
  let mockRouter = createMockObject(routerMethods);
  let mockActivatedRoute: Partial<ActivatedRoute>;

  beforeEach(async () => {
    mockCircleService = createMockObject(circleServiceMethods);
    mockRouter = createMockObject(routerMethods);
    mockActivatedRoute = {
      snapshot: {
        paramMap: {
          get: vi.fn().mockReturnValue('123'),
        },
      } as any,
    };

    await TestBed.configureTestingModule({
      imports: [JoinCircleComponent],
      providers: [
        {
          provide: CircleService,
          useValue: mockCircleService as unknown as CircleService,
        },
        { provide: Router, useValue: mockRouter as unknown as Router },
        { provide: ActivatedRoute, useValue: mockActivatedRoute },
      ],
    }).compileComponents();

    fixture = TestBed.createComponent(JoinCircleComponent);
    component = fixture.componentInstance;
  });

  it('should create', () => {
    expect(component).toBeTruthy();
  });

  it('should successfully join a circle', () => {
    mockCircleService.joinCircle.mockReturnValue(of(undefined));

    fixture.detectChanges();

    expect(mockCircleService.joinCircle).toHaveBeenCalledWith(123);
    expect(component['loading']()).toBe(false);
    expect(component['success']()).toBe(true);
    expect(component['error']()).toBeNull();
  });

  it('should handle join circle error', () => {
    const errorResponse = { error: { error: 'User already a member' } };
    mockCircleService.joinCircle.mockReturnValue(
      throwError(() => errorResponse),
    );

    fixture.detectChanges();

    expect(mockCircleService.joinCircle).toHaveBeenCalledWith(123);
    expect(component['loading']()).toBe(false);
    expect(component['success']()).toBe(false);
    expect(component['error']()).toBe('User already a member');
  });

  it('should navigate to circle detail on viewCircle', () => {
    mockCircleService.joinCircle.mockReturnValue(of(undefined));
    fixture.detectChanges();

    component['viewCircle']();

    expect(mockRouter.navigate).toHaveBeenCalledWith(['/circles', 123]);
  });

  it('should navigate to circles list on goToCircles', () => {
    mockCircleService.joinCircle.mockReturnValue(
      throwError(() => new Error('test error')),
    );
    fixture.detectChanges();

    component['goToCircles']();

    expect(mockRouter.navigate).toHaveBeenCalledWith(['/circles']);
  });
});
