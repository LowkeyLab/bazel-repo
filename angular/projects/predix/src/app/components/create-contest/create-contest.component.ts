import {
  ChangeDetectionStrategy,
  Component,
  signal,
  inject,
  OnInit,
} from '@angular/core';
import { Router, ActivatedRoute } from '@angular/router';
import { FormsModule } from '@angular/forms';
import { ContestService } from '../../services/contest.service';
import { AuthService } from '../../services/auth.service';

@Component({
  selector: 'app-create-contest',
  imports: [FormsModule],
  template: `
    <div class="container mx-auto px-4 py-8 max-w-3xl">
      <div class="mb-6">
        <button class="btn btn-ghost btn-sm" (click)="goBack()">← Back</button>
      </div>

      <div class="card bg-base-100 shadow-xl">
        <div class="card-body space-y-6">
          <h1 class="card-title text-3xl">Create New Contest</h1>

          <form (submit)="onSubmit($event)" class="space-y-5">
            <div class="space-y-2">
              <span class="label-text text-sm">Subject</span>
              <label
                class="input input-bordered flex items-center gap-3"
                aria-label="Subject"
              >
                <svg
                  class="h-[1em] opacity-60"
                  xmlns="http://www.w3.org/2000/svg"
                  viewBox="0 0 24 24"
                  aria-hidden="true"
                >
                  <g
                    stroke-linejoin="round"
                    stroke-linecap="round"
                    stroke-width="2"
                    fill="none"
                    stroke="currentColor"
                  >
                    <path d="M4 5h16M4 12h8M4 19h6"></path>
                  </g>
                </svg>
                <input
                  type="text"
                  class="grow"
                  placeholder="Who will win the Mario Kart tournament?"
                  [(ngModel)]="question"
                  name="question"
                  required
                />
              </label>
              <p class="text-xs text-secondary">
                What are you making predictions about?
              </p>
            </div>

            <div class="space-y-2">
              <span class="label-text text-sm">Circle</span>
              <label
                class="input input-bordered flex items-center gap-3"
                aria-label="Circle"
              >
                <svg
                  class="h-[1em] opacity-60"
                  xmlns="http://www.w3.org/2000/svg"
                  viewBox="0 0 24 24"
                  aria-hidden="true"
                >
                  <g
                    stroke-linejoin="round"
                    stroke-linecap="round"
                    stroke-width="2"
                    fill="none"
                    stroke="currentColor"
                  >
                    <circle cx="7" cy="7" r="3"></circle>
                    <circle cx="17" cy="7" r="3"></circle>
                    <path
                      d="M4 17c1.3-2 3.7-3 6-3s4.7 1 6 3M14 17l3-3 3 3"
                    ></path>
                  </g>
                </svg>
                <input
                  type="text"
                  class="grow"
                  placeholder="Circle name"
                  [(ngModel)]="circleName"
                  name="circleName"
                  disabled
                />
                <span class="badge badge-neutral badge-xs">Required</span>
              </label>
              <p class="text-xs text-secondary">
                Scope this contest to the circles allowed to stake Clout.
              </p>
            </div>

            <div class="divider">Options</div>

            <div class="space-y-3">
              @for (option of options(); track $index) {
                <div class="flex gap-2 items-stretch">
                  <label
                    class="input input-bordered flex items-center gap-3 flex-1"
                    [attr.aria-label]="'Option ' + ($index + 1)"
                  >
                    <svg
                      class="h-[1em] opacity-60"
                      xmlns="http://www.w3.org/2000/svg"
                      viewBox="0 0 24 24"
                      aria-hidden="true"
                    >
                      <g
                        stroke-linejoin="round"
                        stroke-linecap="round"
                        stroke-width="2"
                        fill="none"
                        stroke="currentColor"
                      >
                        <circle cx="12" cy="12" r="9"></circle>
                        <text
                          x="12"
                          y="16"
                          text-anchor="middle"
                          font-size="10"
                          fill="currentColor"
                          aria-hidden="true"
                        >
                          {{ $index + 1 }}
                        </text>
                      </g>
                    </svg>
                    <input
                      type="text"
                      class="grow"
                      [placeholder]="'Option ' + ($index + 1)"
                      [(ngModel)]="option.value"
                      [name]="'option' + $index"
                      required
                    />
                  </label>
                  @if (options().length > 2) {
                    <button
                      type="button"
                      class="btn btn-square btn-ghost"
                      (click)="removeOption($index)"
                      aria-label="Remove option"
                    >
                      <svg
                        xmlns="http://www.w3.org/2000/svg"
                        class="h-6 w-6"
                        fill="none"
                        viewBox="0 0 24 24"
                        stroke="currentColor"
                        aria-hidden="true"
                      >
                        <path
                          stroke-linecap="round"
                          stroke-linejoin="round"
                          stroke-width="2"
                          d="M6 18L18 6M6 6l12 12"
                        />
                      </svg>
                    </button>
                  }
                </div>
              }
            </div>

            <button
              type="button"
              class="btn btn-sm btn-outline"
              (click)="addOption()"
            >
              + Add Option
            </button>

            <div class="space-y-2">
              <span class="label-text text-sm">Expires at</span>
              <label
                class="input input-bordered flex items-center gap-3"
                aria-label="Expires at"
              >
                <svg
                  class="h-[1em] opacity-60"
                  xmlns="http://www.w3.org/2000/svg"
                  viewBox="0 0 24 24"
                  aria-hidden="true"
                >
                  <g
                    stroke-linejoin="round"
                    stroke-linecap="round"
                    stroke-width="2"
                    fill="none"
                    stroke="currentColor"
                  >
                    <circle cx="12" cy="12" r="9"></circle>
                    <path d="M12 7v5l3 3"></path>
                  </g>
                </svg>
                <input
                  type="datetime-local"
                  class="grow"
                  [(ngModel)]="expiresAt"
                  name="expiresAt"
                  required
                />
              </label>
              <p class="text-xs text-secondary">
                When should predictions close?
              </p>
            </div>

            @if (error()) {
              <div class="alert alert-error">
                <svg
                  xmlns="http://www.w3.org/2000/svg"
                  class="stroke-current shrink-0 h-6 w-6"
                  fill="none"
                  viewBox="0 0 24 24"
                >
                  <path
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    stroke-width="2"
                    d="M10 14l2-2m0 0l2-2m-2 2l-2-2m2 2l2 2m7-2a9 9 0 11-18 0 9 9 0 0118 0z"
                  />
                </svg>
                <span>{{ error() }}</span>
              </div>
            }

            <div class="card-actions justify-end pt-2">
              <button type="button" class="btn btn-ghost" (click)="goBack()">
                Cancel
              </button>
              <button
                type="submit"
                class="btn btn-primary"
                [disabled]="loading()"
              >
                @if (loading()) {
                  <span class="loading loading-spinner"></span>
                }
                Create Contest
              </button>
            </div>
          </form>
        </div>
      </div>
    </div>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class CreateContestComponent implements OnInit {
  private readonly router = inject(Router);
  private readonly route = inject(ActivatedRoute);
  private readonly contestService = inject(ContestService);
  protected readonly auth = inject(AuthService);

  protected question = '';
  protected circleName = '';
  protected circleId: number | null = null;
  protected expiresAt = '';
  protected readonly options = signal([{ value: '' }, { value: '' }]);
  protected readonly loading = signal(false);
  protected readonly error = signal('');

  ngOnInit(): void {
    const id = this.route.snapshot.paramMap.get('id');
    const name = this.route.snapshot.queryParamMap.get('circleName');

    if (id) {
      this.circleId = Number(id);
    }
    if (name) {
      this.circleName = name;
    }
  }

  protected addOption(): void {
    this.options.update((opts) => [...opts, { value: '' }]);
  }

  protected removeOption(index: number): void {
    this.options.update((opts) => opts.filter((_, i) => i !== index));
  }

  protected onSubmit(event: Event): void {
    event.preventDefault();

    // Validate
    if (!this.question.trim()) {
      this.error.set('Question is required');
      return;
    }

    if (!this.circleName.trim()) {
      this.error.set('Circle is required');
      return;
    }

    const optionTexts = this.options()
      .map((opt) => opt.value.trim())
      .filter((text) => text.length > 0);

    if (optionTexts.length < 2) {
      this.error.set('At least 2 options are required');
      return;
    }

    if (!this.expiresAt) {
      this.error.set('Expiration date is required');
      return;
    }

    this.loading.set(true);
    this.error.set('');

    const circleIds = this.circleId ? [this.circleId] : [];

    this.contestService
      .createContest({
        circle_ids: circleIds,
        question: this.question,
        options: optionTexts,
        expires_at: new Date(this.expiresAt).toISOString(),
      })
      .subscribe({
        next: (contest) => {
          this.loading.set(false);
          this.router.navigate(['/contests', contest.id]);
        },
        error: (err) => {
          this.loading.set(false);
          this.error.set(err.error?.error || 'Failed to create contest');
        },
      });
  }

  protected goBack(): void {
    if (this.circleId) {
      this.router.navigate(['/circles', this.circleId]);
    } else {
      this.router.navigate(['/contests']);
    }
  }
}
