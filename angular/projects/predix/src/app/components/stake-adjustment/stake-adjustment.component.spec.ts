import { ComponentFixture, TestBed } from '@angular/core/testing';
import { StakeAdjustmentComponent } from './stake-adjustment.component';
import { Component, signal } from '@angular/core';
import { DebugElement } from '@angular/core';
import { By } from '@angular/platform-browser';

// Wrapper component for testing
@Component({
  selector: 'test-wrapper',
  template: `
    <stake-adjustment
      [config]="config()"
      (stakeChanged)="onStakeChanged($event)"
      (predictionSubmitted)="onPredictionSubmitted()"
    />
  `,
  imports: [StakeAdjustmentComponent],
  standalone: true,
})
class TestWrapperComponent {
  config = signal({ minStake: 100, currentStake: 1000, isLoading: false });
  stakeChangedValue: number | null = null;
  predictionSubmittedCalled = false;

  onStakeChanged(value: number): void {
    this.stakeChangedValue = value;
  }

  onPredictionSubmitted(): void {
    this.predictionSubmittedCalled = true;
  }
}

describe('StakeAdjustmentComponent', () => {
  let wrapperComponent: TestWrapperComponent;
  let wrapperFixture: ComponentFixture<TestWrapperComponent>;

  beforeEach(async () => {
    await TestBed.configureTestingModule({
      imports: [TestWrapperComponent],
    }).compileComponents();

    wrapperFixture = TestBed.createComponent(TestWrapperComponent);
    wrapperComponent = wrapperFixture.componentInstance;
    wrapperFixture.detectChanges();
  });

  describe('Component Creation', () => {
    it('should create', () => {
      expect(wrapperComponent).toBeTruthy();
    });

    it('should render with config values', () => {
      const input: DebugElement = wrapperFixture.debugElement.query(
        By.css('input[type="number"]'),
      );
      expect(input).toBeTruthy();
      expect(input.nativeElement.value).toBe('1000');
    });
  });

  describe('UI Rendering', () => {
    it('should have 6 adjustment buttons', () => {
      const adjustmentButtons = wrapperFixture.debugElement.queryAll(
        By.css('button'),
      );
      // 6 adjustment buttons + 1 submit button
      expect(adjustmentButtons.length).toBe(7);
    });

    it('should have adjustment buttons with correct labels', () => {
      const buttons = wrapperFixture.debugElement.queryAll(By.css('button'));
      const labels = buttons.map((b) => b.nativeElement.textContent.trim());

      expect(labels).toContain('-100');
      expect(labels).toContain('-1000');
      expect(labels).toContain('-10000');
      expect(labels).toContain('+100');
      expect(labels).toContain('+1000');
      expect(labels).toContain('+10000');
    });

    it('should have a submit button with correct text', () => {
      const buttons = wrapperFixture.debugElement.queryAll(By.css('button'));
      const submitButton = buttons[buttons.length - 1];
      expect(submitButton.nativeElement.textContent).toContain(
        'Place Prediction',
      );
    });

    it('should display minimum stake requirement', () => {
      const text = wrapperFixture.nativeElement.textContent;
      expect(text).toContain('Min 100 Clout');
    });

    it('should display current minimum stake in label', () => {
      wrapperComponent.config.set({
        minStake: 500,
        currentStake: 1000,
        isLoading: false,
      });
      wrapperFixture.detectChanges();

      const text = wrapperFixture.nativeElement.textContent;
      expect(text).toContain('Min 500 Clout');
    });
  });

  describe('Stake Updates via Input', () => {
    it('should emit stakeChanged when input value changes', async () => {
      const input = wrapperFixture.debugElement.query(
        By.css('input[type="number"]'),
      ).nativeElement;

      input.value = '2000';
      input.dispatchEvent(new Event('input'));
      wrapperFixture.detectChanges();

      await wrapperFixture.whenStable();
      expect(wrapperComponent.stakeChangedValue).toBe(2000);
    });

    it('should update displayed stake when config changes', () => {
      const input = wrapperFixture.debugElement.query(
        By.css('input[type="number"]'),
      ).nativeElement;
      expect(input.value).toBe('1000');

      wrapperComponent.config.set({
        minStake: 100,
        currentStake: 5000,
        isLoading: false,
      });
      wrapperFixture.detectChanges();

      expect(input.value).toBe('5000');
    });

    it('should update minimum stake attribute when config changes', () => {
      const input = wrapperFixture.debugElement.query(
        By.css('input[type="number"]'),
      ).nativeElement;

      wrapperComponent.config.set({
        minStake: 500,
        currentStake: 1000,
        isLoading: false,
      });
      wrapperFixture.detectChanges();

      expect(input.min).toBe('500');
    });
  });

  describe('Stake Adjustment via Buttons', () => {
    it('should adjust stake up by 100', async () => {
      const buttons = wrapperFixture.debugElement.queryAll(By.css('button'));
      // +100 is the 4th button
      buttons[3].nativeElement.click();
      wrapperFixture.detectChanges();

      await wrapperFixture.whenStable();
      expect(wrapperComponent.stakeChangedValue).toBe(1100);
    });

    it('should adjust stake down by 100', async () => {
      const buttons = wrapperFixture.debugElement.queryAll(By.css('button'));
      // -100 is the 1st button
      buttons[0].nativeElement.click();
      wrapperFixture.detectChanges();

      await wrapperFixture.whenStable();
      expect(wrapperComponent.stakeChangedValue).toBe(900);
    });

    it('should adjust stake up by 1000', async () => {
      const buttons = wrapperFixture.debugElement.queryAll(By.css('button'));
      // +1000 is the 5th button
      buttons[4].nativeElement.click();
      wrapperFixture.detectChanges();

      await wrapperFixture.whenStable();
      expect(wrapperComponent.stakeChangedValue).toBe(2000);
    });

    it('should adjust stake down by 1000', async () => {
      const buttons = wrapperFixture.debugElement.queryAll(By.css('button'));
      // -1000 is the 2nd button
      buttons[1].nativeElement.click();
      wrapperFixture.detectChanges();

      await wrapperFixture.whenStable();
      expect(wrapperComponent.stakeChangedValue).toBe(100);
    });

    it('should clamp to minimum stake when adjusting down', async () => {
      wrapperComponent.config.set({
        minStake: 100,
        currentStake: 150,
        isLoading: false,
      });
      wrapperFixture.detectChanges();

      const buttons = wrapperFixture.debugElement.queryAll(By.css('button'));
      // -100 is the 1st button
      buttons[0].nativeElement.click();
      wrapperFixture.detectChanges();

      await wrapperFixture.whenStable();
      expect(wrapperComponent.stakeChangedValue).toBe(100);
    });

    it('should clamp large decrements to minimum stake', async () => {
      wrapperComponent.config.set({
        minStake: 100,
        currentStake: 500,
        isLoading: false,
      });
      wrapperFixture.detectChanges();

      const buttons = wrapperFixture.debugElement.queryAll(By.css('button'));
      // -10000 is the 3rd button - much larger than current stake
      buttons[2].nativeElement.click();
      wrapperFixture.detectChanges();

      await wrapperFixture.whenStable();
      expect(wrapperComponent.stakeChangedValue).toBe(100);
    });
  });

  describe('Button States', () => {
    it('should disable decrease buttons when stake equals minimum', () => {
      wrapperComponent.config.set({
        minStake: 1000,
        currentStake: 1000,
        isLoading: false,
      });
      wrapperFixture.detectChanges();

      const decreaseButtons = wrapperFixture.debugElement.queryAll(
        By.css('button'),
      );
      // First 3 buttons are decrease buttons
      expect(decreaseButtons[0].nativeElement.disabled).toBe(true);
      expect(decreaseButtons[1].nativeElement.disabled).toBe(true);
      expect(decreaseButtons[2].nativeElement.disabled).toBe(true);
    });

    it('should enable decrease buttons when stake is above minimum', () => {
      wrapperComponent.config.set({
        minStake: 100,
        currentStake: 500,
        isLoading: false,
      });
      wrapperFixture.detectChanges();

      const decreaseButtons = wrapperFixture.debugElement.queryAll(
        By.css('button'),
      );
      // First 3 buttons are decrease buttons
      expect(decreaseButtons[0].nativeElement.disabled).toBe(false);
      expect(decreaseButtons[1].nativeElement.disabled).toBe(false);
      expect(decreaseButtons[2].nativeElement.disabled).toBe(false);
    });

    it('should enable increase buttons always', () => {
      const buttons = wrapperFixture.debugElement.queryAll(By.css('button'));
      // Buttons 3, 4, 5 are increase buttons
      expect(buttons[3].nativeElement.disabled).toBe(false);
      expect(buttons[4].nativeElement.disabled).toBe(false);
      expect(buttons[5].nativeElement.disabled).toBe(false);
    });

    it('should disable all buttons when loading', () => {
      wrapperComponent.config.set({
        minStake: 100,
        currentStake: 1000,
        isLoading: true,
      });
      wrapperFixture.detectChanges();

      const input = wrapperFixture.debugElement.query(
        By.css('input[type="number"]'),
      ).nativeElement;
      expect(input.disabled).toBe(true);

      const buttons = wrapperFixture.debugElement.queryAll(By.css('button'));
      buttons.forEach((button) => {
        expect(button.nativeElement.disabled).toBe(true);
      });
    });

    it('should show loading spinner on submit button during loading', () => {
      wrapperComponent.config.set({
        minStake: 100,
        currentStake: 1000,
        isLoading: true,
      });
      wrapperFixture.detectChanges();

      const spinner = wrapperFixture.debugElement.query(
        By.css('.loading.loading-spinner'),
      );
      expect(spinner).toBeTruthy();
    });
  });

  describe('Form Submission', () => {
    it('should emit predictionSubmitted when submit button clicked', async () => {
      const submitButton = wrapperFixture.debugElement.queryAll(
        By.css('button'),
      );
      const placeButton = submitButton[submitButton.length - 1]; // Last button is submit

      placeButton.nativeElement.click();
      wrapperFixture.detectChanges();

      await wrapperFixture.whenStable();
      expect(wrapperComponent.predictionSubmittedCalled).toBe(true);
    });

    it('should emit predictionSubmitted once per click', async () => {
      let emitCount = 0;
      wrapperComponent.onPredictionSubmitted = () => {
        emitCount++;
      };

      const submitButton = wrapperFixture.debugElement.queryAll(
        By.css('button'),
      );
      const placeButton = submitButton[submitButton.length - 1];

      placeButton.nativeElement.click();
      wrapperFixture.detectChanges();

      await wrapperFixture.whenStable();
      expect(emitCount).toBe(1);
    });
  });

  describe('User Prediction Prefill (from parent)', () => {
    it('should display existing user prediction stake', () => {
      wrapperComponent.config.set({
        minStake: 100,
        currentStake: 500, // Existing stake
        isLoading: false,
      });
      wrapperFixture.detectChanges();

      const input = wrapperFixture.debugElement.query(
        By.css('input[type="number"]'),
      ).nativeElement;
      expect(input.value).toBe('500');
    });

    it('should use minimum stake as default when no prediction exists', () => {
      wrapperComponent.config.set({
        minStake: 100,
        currentStake: 100,
        isLoading: false,
      });
      wrapperFixture.detectChanges();

      const input = wrapperFixture.debugElement.query(
        By.css('input[type="number"]'),
      ).nativeElement;
      expect(input.value).toBe('100');
    });
  });

  describe('Integration Scenarios', () => {
    it('should handle rapid stake changes', async () => {
      const input = wrapperFixture.debugElement.query(
        By.css('input[type="number"]'),
      ).nativeElement;

      // Simulate rapid input changes
      input.value = '1500';
      input.dispatchEvent(new Event('input'));
      wrapperFixture.detectChanges();

      input.value = '2000';
      input.dispatchEvent(new Event('input'));
      wrapperFixture.detectChanges();

      input.value = '2500';
      input.dispatchEvent(new Event('input'));
      wrapperFixture.detectChanges();

      await wrapperFixture.whenStable();
      expect(wrapperComponent.stakeChangedValue).toBe(2500);
    });

    it('should handle mixed input and button adjustments', async () => {
      const input = wrapperFixture.debugElement.query(
        By.css('input[type="number"]'),
      ).nativeElement;

      // Direct input (emits event with 2000)
      input.value = '2000';
      input.dispatchEvent(new Event('input'));
      wrapperFixture.detectChanges();

      await wrapperFixture.whenStable();
      expect(wrapperComponent.stakeChangedValue).toBe(2000);

      wrapperComponent.config.set({
        minStake: 100,
        currentStake: 2000,
        isLoading: false,
      });
      wrapperFixture.detectChanges();

      const buttons = wrapperFixture.debugElement.queryAll(By.css('button'));
      buttons[4].nativeElement.click();
      wrapperFixture.detectChanges();

      await wrapperFixture.whenStable();
      expect(wrapperComponent.stakeChangedValue).toBe(3000);
    });
  });
});
