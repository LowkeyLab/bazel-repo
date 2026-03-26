import {
  ChangeDetectionStrategy,
  Component,
  inject,
  input,
  signal,
} from '@angular/core';
import { FormsModule } from '@angular/forms';
import { RouterLink } from '@angular/router';
import { FAILSAFE_SCHEMA, load } from 'js-yaml';
import { CreateNamesGQL } from '../../generated/graphql';

interface NameEntry {
  discordId: string;
  name: string;
}

@Component({
  selector: 'app-batch-add-names',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [FormsModule, RouterLink],
  template: `
    <div class="p-4">
      <a
        [routerLink]="'/servers/' + serverId() + '/names'"
        class="link link-hover mb-4 inline-block"
      >
        &larr; Back to names
      </a>

      <h1 class="text-2xl font-bold mb-4">
        Batch Add Names — Server {{ serverId() }}
      </h1>

      <form (ngSubmit)="onSubmit()" data-testid="batch-form">
        <label class="form-control mb-4">
          <span class="label-text mb-1">Paste YAML (discord ID: name)</span>
          <textarea
            class="textarea textarea-bordered font-mono h-64 w-full max-w-2xl"
            [ngModel]="yamlInput()"
            (ngModelChange)="yamlInput.set($event)"
            name="yamlInput"
            placeholder="123456789012345678: Alice&#10;987654321098765432: Bob"
            data-testid="yaml-input"
          ></textarea>
        </label>

        <button
          type="submit"
          class="btn btn-primary"
          [disabled]="submitting() || !yamlInput()"
          data-testid="submit-batch"
        >
          @if (submitting()) {
            <span class="loading loading-spinner loading-sm"></span>
          }
          Submit Batch
        </button>
      </form>

      @if (error()) {
        <div class="alert alert-error mt-4" data-testid="batch-error">
          {{ error() }}
        </div>
      }

      @if (successCount() !== null) {
        <div class="alert alert-success mt-4" data-testid="batch-success">
          Successfully upserted {{ successCount() }} name(s).
        </div>
      }
    </div>
  `,
})
export class BatchAddNamesComponent {
  readonly serverId = input.required<string>();

  private readonly createNamesGQL = inject(CreateNamesGQL);

  protected readonly yamlInput = signal('');
  protected readonly submitting = signal(false);
  protected readonly error = signal<string | null>(null);
  protected readonly successCount = signal<number | null>(null);

  private parseYaml(raw: string): NameEntry[] {
    const parsed = load(raw, { schema: FAILSAFE_SCHEMA });
    if (
      typeof parsed !== 'object' ||
      parsed === null ||
      Array.isArray(parsed)
    ) {
      throw new Error('YAML must be a mapping of discord IDs to names');
    }
    const map = parsed as Record<string, unknown>;
    const entries = Object.entries(map);
    return entries.map(([key, value]) => {
      if (!/^\d+$/.test(key)) {
        throw new Error(
          `Entry '${key}': invalid Discord ID (must be a number)`,
        );
      }
      if (typeof value !== 'string' || value.trim() === '') {
        throw new Error(`Entry '${key}': missing or invalid name`);
      }
      return { discordId: key, name: value };
    });
  }

  protected onSubmit(): void {
    this.error.set(null);
    this.successCount.set(null);

    let entries: NameEntry[];
    try {
      entries = this.parseYaml(this.yamlInput());
    } catch (e) {
      this.error.set(e instanceof Error ? e.message : 'Invalid YAML');
      return;
    }

    if (entries.length === 0) {
      this.error.set('No entries found in YAML');
      return;
    }

    this.submitting.set(true);

    this.createNamesGQL
      .mutate({
        variables: {
          input: {
            discordServerId: this.serverId(),
            names: entries,
          },
        },
      })
      .subscribe({
        next: ({ data }) => {
          this.submitting.set(false);
          const count = data?.createNames?.names?.length ?? entries.length;
          this.successCount.set(count);
          this.yamlInput.set('');
        },
        error: (err: Error) => {
          this.error.set(err.message);
          this.submitting.set(false);
        },
      });
  }
}
