import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  inject,
  signal,
} from '@angular/core';
import { takeUntilDestroyed } from '@angular/core/rxjs-interop';
import { FormsModule } from '@angular/forms';
import { Router } from '@angular/router';
import { CreateServerGQL } from '../../generated/graphql';

@Component({
  selector: 'app-add-server',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [FormsModule],
  template: `
    <div class="p-4 max-w-xl">
      <h1 class="text-2xl font-bold mb-4">Add Server</h1>

      <form
        class="flex flex-col gap-4"
        (ngSubmit)="onSubmit()"
        data-testid="add-server-form"
      >
        <label class="form-control w-full">
          <span class="label-text">Discord Server ID</span>
          <input
            type="text"
            class="input input-bordered w-full"
            [ngModel]="serverId()"
            (ngModelChange)="serverId.set($event)"
            name="serverId"
            required
            data-testid="server-id-input"
          />
        </label>
        <label class="form-control w-full">
          <span class="label-text">Display Name</span>
          <input
            type="text"
            class="input input-bordered w-full"
            [ngModel]="displayName()"
            (ngModelChange)="displayName.set($event)"
            name="displayName"
            required
            data-testid="display-name-input"
          />
        </label>
        <button
          type="submit"
          class="btn btn-primary"
          [disabled]="submitting() || !serverId() || !displayName()"
          data-testid="submit-server"
        >
          @if (submitting()) {
            <span class="loading loading-spinner loading-sm"></span>
          }
          Create Server
        </button>
      </form>

      @if (error()) {
        <div class="alert alert-error mt-4" data-testid="submit-error">
          {{ error() }}
        </div>
      }
    </div>
  `,
})
export class AddServerComponent {
  private readonly createServerGQL = inject(CreateServerGQL);
  private readonly router = inject(Router);
  private readonly destroyRef = inject(DestroyRef);

  protected readonly serverId = signal('');
  protected readonly displayName = signal('');
  protected readonly submitting = signal(false);
  protected readonly error = signal<string | null>(null);

  protected onSubmit(): void {
    this.submitting.set(true);
    this.error.set(null);

    this.createServerGQL
      .mutate({
        variables: {
          input: {
            discordServerId: this.serverId(),
            displayName: this.displayName(),
          },
        },
      })
      .pipe(takeUntilDestroyed(this.destroyRef))
      .subscribe({
        next: (result) => {
          this.submitting.set(false);
          const serverId = result.data?.createServer?.server?.serverId;
          if (serverId) {
            this.router.navigate(['/servers', serverId, 'names']);
          }
        },
        error: (err: Error) => {
          this.error.set(err.message);
          this.submitting.set(false);
        },
      });
  }
}
