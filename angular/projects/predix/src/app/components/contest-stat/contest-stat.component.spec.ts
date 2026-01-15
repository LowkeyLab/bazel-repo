import { ComponentFixture, TestBed } from '@angular/core/testing';
import { ContestStatComponent } from './contest-stat.component';
import { By } from '@angular/platform-browser';

describe('ContestStatComponent', () => {
  let component: ContestStatComponent;
  let fixture: ComponentFixture<ContestStatComponent>;

  beforeEach(async () => {
    try {
      jasmine.clock().uninstall();
    } catch (e) {}
    jasmine.clock().install();

    await TestBed.configureTestingModule({
      imports: [ContestStatComponent],
    }).compileComponents();

    fixture = TestBed.createComponent(ContestStatComponent);
    component = fixture.componentInstance;

    // Set required inputs
    fixture.componentRef.setInput('title', 'Test Stat');
    fixture.componentRef.setInput('value', 100);
    fixture.detectChanges();
  });

  afterEach(() => {
    jasmine.clock().uninstall();
  });

  it('should create', () => {
    expect(component).toBeTruthy();
  });

  it('should display title and value', () => {
    const titleEl = fixture.debugElement.query(
      By.css('.stat-title'),
    ).nativeElement;
    const valueEl = fixture.debugElement.query(
      By.css('.stat-value'),
    ).nativeElement;

    expect(titleEl.textContent).toContain('Test Stat');
    expect(valueEl.textContent).toContain('100');
  });

  it('should flash green (up) when value increases', () => {
    // Initial state
    const valueEl = fixture.debugElement.query(By.css('.stat-value'));
    expect(valueEl.classes['text-green-500']).toBeFalsy();
    expect(valueEl.classes['text-red-500']).toBeFalsy();

    // Increase value
    fixture.componentRef.setInput('value', 150);
    fixture.detectChanges();

    // Effect should have run
    expect(valueEl.classes['text-green-500']).toBe(true);
    expect(valueEl.classes['text-red-500']).toBeFalsy();

    // Wait for flash duration
    jasmine.clock().tick(1000);
    fixture.detectChanges();

    expect(valueEl.classes['text-green-500']).toBeFalsy();
  });

  it('should flash red (down) when value decreases', () => {
    // Initial state
    const valueEl = fixture.debugElement.query(By.css('.stat-value'));
    expect(valueEl.classes['text-green-500']).toBeFalsy();
    expect(valueEl.classes['text-red-500']).toBeFalsy();

    // Decrease value
    fixture.componentRef.setInput('value', 50);
    fixture.detectChanges();

    // Effect should have run
    expect(valueEl.classes['text-green-500']).toBeFalsy();
    expect(valueEl.classes['text-red-500']).toBe(true);

    // Wait for flash duration
    jasmine.clock().tick(1000);
    fixture.detectChanges();

    expect(valueEl.classes['text-red-500']).toBeFalsy();
  });
});
