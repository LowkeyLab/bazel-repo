import { ComponentFixture, TestBed } from '@angular/core/testing';
import { JoinCircleComponent } from './join-circle.component';
import { ActivatedRoute, Router } from '@angular/router';
import { CircleService } from '../../services/circle.service';
import { of, throwError } from 'rxjs';

describe('JoinCircleComponent', () => {
  let component: JoinCircleComponent;
  let fixture: ComponentFixture<JoinCircleComponent>;
  let mockCircleService: jasmine.SpyObj<CircleService>;
  let mockRouter: jasmine.SpyObj<Router>;
  let mockActivatedRoute: Partial<ActivatedRoute>;

  beforeEach(async () => {
    mockCircleService = jasmine.createSpyObj('CircleService', ['joinCircle']);
    mockRouter = jasmine.createSpyObj('Router', ['navigate']);
    mockActivatedRoute = {
      snapshot: {
        paramMap: {
          get: jasmine.createSpy('get').and.returnValue('123'),
        },
      } as any,
    };

    await TestBed.configureTestingModule({
      imports: [JoinCircleComponent],
      providers: [
        { provide: CircleService, useValue: mockCircleService },
        { provide: Router, useValue: mockRouter },
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
    mockCircleService.joinCircle.and.returnValue(of(undefined));

    fixture.detectChanges();

    expect(mockCircleService.joinCircle).toHaveBeenCalledWith(123);
    expect(component['loading']()).toBe(false);
    expect(component['success']()).toBe(true);
    expect(component['error']()).toBeNull();
  });

  it('should handle join circle error', () => {
    const errorResponse = { error: { error: 'User already a member' } };
    mockCircleService.joinCircle.and.returnValue(
      throwError(() => errorResponse),
    );

    fixture.detectChanges();

    expect(mockCircleService.joinCircle).toHaveBeenCalledWith(123);
    expect(component['loading']()).toBe(false);
    expect(component['success']()).toBe(false);
    expect(component['error']()).toBe('User already a member');
  });

  it('should navigate to circle detail on viewCircle', () => {
    mockCircleService.joinCircle.and.returnValue(of(undefined));
    fixture.detectChanges();

    component['viewCircle']();

    expect(mockRouter.navigate).toHaveBeenCalledWith(['/circles', 123]);
  });

  it('should navigate to circles list on goToCircles', () => {
    mockCircleService.joinCircle.and.returnValue(
      throwError(() => new Error('test error')),
    );
    fixture.detectChanges();

    component['goToCircles']();

    expect(mockRouter.navigate).toHaveBeenCalledWith(['/circles']);
  });
});
