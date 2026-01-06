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
  selector: 'circle-detail',
  imports: [DatePipe, BackButtonComponent],
  templateUrl: './circle-detail.component.html',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class CircleDetailComponent implements OnInit {
  private readonly route = inject(ActivatedRoute);
  private readonly router = inject(Router);
  private readonly circleService = inject(CircleService);
  private readonly document = inject(DOCUMENT);

  public readonly circle = signal<Circle | null>(null);
  public readonly loading = signal(true);
  public readonly contests = signal<Contest[]>([]);
  public readonly loadingContests = signal(false);
  public readonly linkCopied = signal(false);

  ngOnInit(): void {
    const id = Number(this.route.snapshot.paramMap.get('id'));
    if (id) {
      this.loadCircle(id);
      this.loadContests(id);
    }
  }

  public loadCircle(id: number): void {
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

  public loadContests(circleId: number): void {
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

  public viewContest(id: number): void {
    const circleId = this.circle()?.id;
    if (circleId) {
      this.router.navigate(['/circles', circleId, 'contests', id]);
    }
  }

  public createContest(circleName: string): void {
    this.router.navigate(['/circles', this.circle()?.id, 'contest', 'new'], {
      queryParams: { circleName },
    });
  }

  public addMember(): void {
    // TODO: Implement add member modal/form
    alert('Add member functionality coming soon!');
  }

  public getMaxClout(circle: Circle): number {
    return Math.max(...circle.members.map((m) => m.clout));
  }

  public getJoinLink(circleId: number): string {
    const origin = this.document.location.origin;
    return `${origin}/circles/${circleId}/join`;
  }

  public copyJoinLink(circleId: number): void {
    const link = this.getJoinLink(circleId);
    navigator.clipboard.writeText(link).then(() => {
      this.linkCopied.set(true);
      setTimeout(() => this.linkCopied.set(false), 2000);
    });
  }
}
