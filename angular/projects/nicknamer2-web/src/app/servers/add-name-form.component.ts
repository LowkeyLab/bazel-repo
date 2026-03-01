import {
  ChangeDetectionStrategy,
  Component,
  input,
  output,
  signal,
} from '@angular/core';
import { FormsModule } from '@angular/forms';

@Component({
  selector: 'app-add-name-form',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [FormsModule],
  template: `
    <form
      class="flex gap-2 mb-4 items-end"
      (ngSubmit)="onSubmit()"
      data-testid="add-name-form"
    >
      <label class="form-control">
        <span class="label-text">Discord ID</span>
        <input
          type="text"
          class="input input-bordered"
          [ngModel]="discordId()"
          (ngModelChange)="discordId.set($event)"
          name="discordId"
          required
          data-testid="discord-id-input"
        />
      </label>
      <label class="form-control">
        <span class="label-text">Nickname</span>
        <input
          type="text"
          class="input input-bordered"
          [ngModel]="nickname()"
          (ngModelChange)="nickname.set($event)"
          name="nickname"
          required
          data-testid="nickname-input"
        />
      </label>
      <button
        type="submit"
        class="btn btn-primary"
        [disabled]="submitting() || !discordId() || !nickname()"
        data-testid="submit-name"
      >
        @if (submitting()) {
          <span class="loading loading-spinner loading-sm"></span>
        }
        Add Name
      </button>
    </form>

    @if (error()) {
      <div class="alert alert-error mb-4" data-testid="submit-error">
        {{ error() }}
      </div>
    }
  `,
})
export class AddNameFormComponent {
  readonly submitting = input(false);
  readonly error = input<string | null>(null);
  readonly nameSubmitted = output<{ discordId: string; name: string }>();

  protected readonly discordId = signal('');
  protected readonly nickname = signal('');

  protected onSubmit(): void {
    this.nameSubmitted.emit({
      discordId: this.discordId(),
      name: this.nickname(),
    });
    this.discordId.set('');
    this.nickname.set('');
  }
}
