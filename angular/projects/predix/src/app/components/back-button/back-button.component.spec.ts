import { ComponentFixture, TestBed } from '@angular/core/testing';
import { Router } from '@angular/router';
import { Location } from '@angular/common';
import { BackButtonComponent } from './back-button.component';
import { Component } from '@angular/core';
import { By } from '@angular/platform-browser';

describe('BackButtonComponent', () => {
  let component: BackButtonComponent;
  let fixture: ComponentFixture<BackButtonComponent>;
  let router: Router;
  let location: Location;

  beforeEach(async () => {
    await TestBed.configureTestingModule({
      imports: [BackButtonComponent],
      providers: [
        {
          provide: Router,
          useValue: {
            navigate: jasmine.createSpy('navigate'),
          },
        },
        {
          provide: Location,
          useValue: {
            back: jasmine.createSpy('back'),
          },
        },
      ],
    }).compileComponents();

    fixture = TestBed.createComponent(BackButtonComponent);
    component = fixture.componentInstance;
    router = TestBed.inject(Router);
    location = TestBed.inject(Location);
  });

  it('should create', () => {
    expect(component).toBeTruthy();
  });

  it('should display default label "Back"', () => {
    fixture.detectChanges();
    const button = fixture.debugElement.query(By.css('button'));
    expect(button.nativeElement.textContent).toContain('Back');
  });

  it('should display custom label when provided', () => {
    TestBed.runInInjectionContext(() => {
      fixture = TestBed.createComponent(BackButtonComponent);
      component = fixture.componentInstance;
    });

    // Create a wrapper component to test with inputs
    @Component({
      selector: 'app-test-wrapper',
      standalone: true,
      imports: [BackButtonComponent],
      template: '<back-button [label]="customLabel" />',
    })
    class TestWrapperComponent {
      customLabel = 'Go Back';
    }

    const wrapperFixture = TestBed.createComponent(TestWrapperComponent);
    wrapperFixture.detectChanges();

    const button = wrapperFixture.debugElement.query(By.css('button'));
    expect(button.nativeElement.textContent).toContain('Go Back');
  });

  it('should call location.back() when no link is provided', () => {
    fixture.detectChanges();
    const button = fixture.debugElement.query(By.css('button'));

    button.nativeElement.click();

    expect(location.back).toHaveBeenCalled();
    expect(router.navigate).not.toHaveBeenCalled();
  });

  it('should navigate to provided link string', () => {
    TestBed.runInInjectionContext(() => {
      fixture = TestBed.createComponent(BackButtonComponent);
      component = fixture.componentInstance;
    });

    @Component({
      selector: 'app-test-wrapper',
      standalone: true,
      imports: [BackButtonComponent],
      template: '<back-button [link]="targetLink" />',
    })
    class TestWrapperComponent {
      targetLink = '/contests';
    }

    const wrapperFixture = TestBed.createComponent(TestWrapperComponent);
    wrapperFixture.detectChanges();

    const button = wrapperFixture.debugElement.query(By.css('button'));
    button.nativeElement.click();

    expect(router.navigate).toHaveBeenCalledWith(['/contests']);
    expect(location.back).not.toHaveBeenCalled();
  });

  it('should navigate to provided link array', () => {
    TestBed.runInInjectionContext(() => {
      fixture = TestBed.createComponent(BackButtonComponent);
      component = fixture.componentInstance;
    });

    @Component({
      selector: 'app-test-wrapper',
      standalone: true,
      imports: [BackButtonComponent],
      template: '<back-button [link]="targetLink" />',
    })
    class TestWrapperComponent {
      targetLink = ['/contests', 'list'];
    }

    const wrapperFixture = TestBed.createComponent(TestWrapperComponent);
    wrapperFixture.detectChanges();

    const button = wrapperFixture.debugElement.query(By.css('button'));
    button.nativeElement.click();

    expect(router.navigate).toHaveBeenCalledWith(['/contests', 'list']);
    expect(location.back).not.toHaveBeenCalled();
  });

  it('should have btn and btn-ghost classes', () => {
    fixture.detectChanges();
    const button = fixture.debugElement.query(By.css('button'));

    expect(button.nativeElement.classList.contains('btn')).toBe(true);
    expect(button.nativeElement.classList.contains('btn-ghost')).toBe(true);
  });
});
