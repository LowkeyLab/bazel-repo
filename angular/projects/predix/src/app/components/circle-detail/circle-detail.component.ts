import {
  ChangeDetectionStrategy,
  Component,
  signal,
  inject,
  OnInit,
} from '@angular/core';
import { ActivatedRoute, Router } from '@angular/router';
import { CircleService } from '../../services/circle.service';
import type { Circle } from '../../models/circle.model';
import type { Contest } from '../../models/contest.model';
import { DatePipe } from '@angular/common';
import { DOCUMENT } from '@angular/common';
import { BackButtonComponent } from '../back-button/back-button.component';

@Component({
  selector: 'app-circle-detail',
  imports: [DatePipe, BackButtonComponent],
  template: `
    <div class="container mx-auto px-4 py-8">
      @if (loading()) {
        <div class="flex justify-center">
          <span class="loading loading-spinner loading-lg"></span>
        </div>
      } @else if (circle(); as circle) {
        <div class="mb-6">
          <app-back-button [link]="'/circles'" [label]="'Back'" />
        </div>

        <div class="card bg-base-100 shadow-xl">
          <div class="card-body">
            <h1 class="card-title text-4xl mb-4">{{ circle.name }}</h1>
            <p class="text-sm text-secondary">
              Created {{ circle.created_at | date: 'medium' }}
            </p>

            <div class="divider"></div>

            <div class="mb-6">
              <h2 class="text-2xl font-bold mb-2">Invite Link</h2>
              <p class="text-sm text-secondary mb-3">
                Share this link with others to invite them to join this circle
              </p>
              <div class="flex gap-2">
                <input
                  type="text"
                  readonly
                  [value]="getJoinLink(circle.id)"
                  class="input input-bordered flex-1 font-mono text-sm"
                  #joinLinkInput
                />
                <button
                  class="btn btn-primary"
                  (click)="copyJoinLink(circle.id)"
                >
                  @if (linkCopied()) {
                    ✓ Copied!
                  } @else {
                    📋 Copy
                  }
                </button>
              </div>
            </div>

            <div class="divider"></div>

            <h2 class="text-2xl font-bold mb-4">Members</h2>
            <div class="overflow-x-auto">
              <table class="table">
                <thead>
                  <tr>
                    <th>Username</th>
                    <th>💎 Clout</th>
                    <th></th>
                  </tr>
                </thead>
                <tbody>
                  @for (member of circle.members; track member.user_id) {
                    <tr>
                      <td>
                        <div class="font-semibold">{{ member.username }}</div>
                      </td>
                      <td>
                        <div class="badge badge-primary">
                          {{ member.clout }}
                        </div>
                      </td>
                      <td>
                        @if (member.clout === getMaxClout(circle)) {
                          <div class="badge badge-warning">👑 Top Dog</div>
                        }
                      </td>
                    </tr>
                  }
                </tbody>
              </table>
            </div>
          </div>
        </div>

        <div class="mt-8">
          <div class="flex items-center justify-between mb-4">
            <h2 class="text-2xl font-bold">Contests</h2>
            <button
              class="btn btn-primary btn-sm"
              (click)="createContest(circle.name)"
            >
              + Create Contest
            </button>
          </div>
          @if (loadingContests()) {
            <div class="flex justify-center">
              <span class="loading loading-spinner loading-lg"></span>
            </div>
          } @else if (contests().length === 0) {
            <div class="alert alert-info">
              <span>No contests in this circle yet.</span>
            </div>
          } @else {
            <div class="grid gap-4">
              @for (contest of contests(); track contest.id) {
                <div
                  class="card bg-base-100 shadow-xl hover:shadow-2xl transition-shadow cursor-pointer"
                  (click)="viewContest(contest.id)"
                >
                  <div class="card-body">
                    <h3 class="card-title text-lg">{{ contest.question }}</h3>
                    <p class="text-sm text-secondary">
                      Expires {{ contest.expires_at | date: 'short' }}
                    </p>
                    <div class="flex gap-2 flex-wrap">
                      @for (option of contest.options; track option.id) {
                        <div class="badge badge-outline">{{ option.text }}</div>
                      }
                    </div>
                    <div class="card-actions justify-between items-center mt-2">
                      <span class="text-sm text-secondary"
                        >{{ contest.predictions.length }} predictions</span
                      >
                      <div class="badge badge-primary">
                        {{ contest.status }}
                      </div>
                    </div>
                  </div>
                </div>
              }
            </div>
          }
        </div>
      } @else {
        <div class="alert alert-error">
          <span>Circle not found</span>
        </div>
      }
    </div>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class CircleDetailComponent implements OnInit {
  private readonly route = inject(ActivatedRoute);
  private readonly router = inject(Router);
  private readonly circleService = inject(CircleService);
  private readonly document = inject(DOCUMENT);

  protected readonly circle = signal<Circle | null>(null);
  protected readonly loading = signal(true);
  protected readonly contests = signal<Contest[]>([]);
  protected readonly loadingContests = signal(false);
  protected readonly linkCopied = signal(false);

  ngOnInit(): void {
    const id = Number(this.route.snapshot.paramMap.get('id'));
    if (id) {
      this.loadCircle(id);
      this.loadContests(id);
    }
  }

  private loadCircle(id: number): void {
    this.circleService.getCircle(id).subscribe({
      next: (circle) => {
        this.circle.set(circle);
        this.loading.set(false);
      },
      error: () => {
        this.loading.set(false);
      },
    });
  }

  private loadContests(circleId: number): void {
    this.loadingContests.set(true);
    this.circleService.getCircleContests(circleId).subscribe({
      next: (contests) => {
        this.contests.set(contests);
        this.loadingContests.set(false);
      },
      error: () => {
        this.loadingContests.set(false);
      },
    });
  }

  protected viewContest(id: number): void {
    this.router.navigate(['/contests', id]);
  }

  protected createContest(circleName: string): void {
    this.router.navigate(['/circles', this.circle()?.id, 'contest', 'new'], {
      queryParams: { circleName },
    });
  }

  protected addMember(): void {
    // TODO: Implement add member modal/form
    alert('Add member functionality coming soon!');
  }

  protected getMaxClout(circle: Circle): number {
    return Math.max(...circle.members.map((m) => m.clout));
  }

  protected getJoinLink(circleId: number): string {
    const origin = this.document.location.origin;
    return `${origin}/circles/${circleId}/join`;
  }

  protected copyJoinLink(circleId: number): void {
    const link = this.getJoinLink(circleId);
    navigator.clipboard.writeText(link).then(() => {
      this.linkCopied.set(true);
      setTimeout(() => this.linkCopied.set(false), 2000);
    });
  }
}
