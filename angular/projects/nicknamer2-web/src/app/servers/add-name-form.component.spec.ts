import { ComponentFixture, TestBed } from '@angular/core/testing';
import { AddNameFormComponent } from './add-name-form.component';

describe('AddNameFormComponent', () => {
  let fixture: ComponentFixture<AddNameFormComponent>;
  let component: AddNameFormComponent;

  beforeEach(async () => {
    await TestBed.configureTestingModule({
      imports: [AddNameFormComponent],
    }).compileComponents();

    fixture = TestBed.createComponent(AddNameFormComponent);
    component = fixture.componentInstance;
  });

  it('should render the form with two inputs and a submit button', () => {
    fixture.detectChanges();

    const discordInput = fixture.nativeElement.querySelector(
      '[data-testid="discord-id-input"]',
    );
    const nicknameInput = fixture.nativeElement.querySelector(
      '[data-testid="nickname-input"]',
    );
    const submitBtn = fixture.nativeElement.querySelector(
      '[data-testid="submit-name"]',
    );

    expect(discordInput).toBeTruthy();
    expect(nicknameInput).toBeTruthy();
    expect(submitBtn).toBeTruthy();
  });

  it('should disable the submit button when inputs are empty', () => {
    fixture.detectChanges();

    const submitBtn: HTMLButtonElement = fixture.nativeElement.querySelector(
      '[data-testid="submit-name"]',
    );
    expect(submitBtn.disabled).toBe(true);
  });

  it('should enable the submit button when both inputs have values', () => {
    component['discordId'].set('123');
    component['nickname'].set('TestName');
    fixture.detectChanges();

    const submitBtn: HTMLButtonElement = fixture.nativeElement.querySelector(
      '[data-testid="submit-name"]',
    );
    expect(submitBtn.disabled).toBe(false);
  });

  it('should disable the submit button when submitting is true', () => {
    fixture.componentRef.setInput('submitting', true);
    component['discordId'].set('123');
    component['nickname'].set('TestName');
    fixture.detectChanges();

    const submitBtn: HTMLButtonElement = fixture.nativeElement.querySelector(
      '[data-testid="submit-name"]',
    );
    expect(submitBtn.disabled).toBe(true);
  });

  it('should emit nameSubmitted on form submit without clearing inputs', () => {
    component['discordId'].set('999');
    component['nickname'].set('NewNickname');
    fixture.detectChanges();

    let emitted: { discordId: string; name: string } | undefined;
    component.nameSubmitted.subscribe((value) => (emitted = value));

    const form: HTMLFormElement = fixture.nativeElement.querySelector(
      '[data-testid="add-name-form"]',
    );
    form.dispatchEvent(new Event('submit'));
    fixture.detectChanges();

    expect(emitted).toEqual({ discordId: '999', name: 'NewNickname' });
    // Form should NOT clear yet — waiting for parent to report success
    expect(component['discordId']()).toBe('999');
    expect(component['nickname']()).toBe('NewNickname');
  });

  it('should clear inputs when submitting transitions from true to false with no error', () => {
    component['discordId'].set('999');
    component['nickname'].set('NewNickname');
    fixture.detectChanges();

    // Simulate parent setting submitting=true
    fixture.componentRef.setInput('submitting', true);
    fixture.detectChanges();

    // Simulate parent setting submitting=false (success)
    fixture.componentRef.setInput('submitting', false);
    fixture.detectChanges();

    expect(component['discordId']()).toBe('');
    expect(component['nickname']()).toBe('');
  });

  it('should NOT clear inputs when submitting transitions to false with an error', () => {
    component['discordId'].set('999');
    component['nickname'].set('NewNickname');
    fixture.detectChanges();

    // Simulate parent setting submitting=true
    fixture.componentRef.setInput('submitting', true);
    fixture.detectChanges();

    // Simulate parent setting submitting=false with error
    fixture.componentRef.setInput('error', 'Something went wrong');
    fixture.componentRef.setInput('submitting', false);
    fixture.detectChanges();

    expect(component['discordId']()).toBe('999');
    expect(component['nickname']()).toBe('NewNickname');
  });

  it('should display error message when error input is set', () => {
    fixture.componentRef.setInput('error', 'Something went wrong');
    fixture.detectChanges();

    const errorDiv = fixture.nativeElement.querySelector(
      '[data-testid="submit-error"]',
    );
    expect(errorDiv).toBeTruthy();
    expect(errorDiv.textContent).toContain('Something went wrong');
  });

  it('should not display error message when error input is null', () => {
    fixture.detectChanges();

    const errorDiv = fixture.nativeElement.querySelector(
      '[data-testid="submit-error"]',
    );
    expect(errorDiv).toBeFalsy();
  });
});
