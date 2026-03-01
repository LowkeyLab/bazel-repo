import {
  ChangeDetectionStrategy,
  Component,
  inject,
  input,
  signal,
} from '@angular/core';
import { FormsModule } from '@angular/forms';
import { RouterLink } from '@angular/router';
import { load } from 'js-yaml';
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
      <a [routerLink]="'/servers/' + serverId() + '/names'" class="link link-hover mb-4 inline-block">
        &larr; Back to names
      </a>

      <h1 class="text-2xl font-bold mb-4">Batch Add Names — Server {{ serverId() }}</h1>

      <form (ngSubmit)="onSubmit()" data-testid="batch-form">
        <label class="form-control mb-4">
          <span class="label-text mb-1">Paste YAML (one entry per item)</span>
          <textarea
            class="textarea textarea-bordered font-mono h-64 w-full max-w-2xl"
            [ngModel]="yamlInput()"
            (ngModelChange)="yamlInput.set($event)"
            name="yamlInput"
            placeholder="- discordId: &quot;123456789&quot;&#10;  name: Alice&#10;- discordId: &quot;987654321&quot;&#10;  name: Bob"
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
    const parsed = load(raw);
    if (!Array.isArray(parsed)) {
      throw new Error('YAML must be a list of entries');
    }
    return parsed.map((item: unknown, i: number) => {
      if (typeof item !== 'object' || item === null) {
        throw new Error(`Entry ${i + 1}: must be an object`);
      }
      const obj = item as Record<string, unknown>;
      if (typeof obj['discordId'] !== 'string' && typeof obj['discordId'] !== 'number') {
        throw new Error(`Entry ${i + 1}: missing or invalid discordId`);
      }
      if (typeof obj['name'] !== 'string') {
        throw new Error(`Entry ${i + 1}: missing or invalid name`);
      }
      return { discordId: String(obj['discordId']), name: String(obj['name']) };
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
