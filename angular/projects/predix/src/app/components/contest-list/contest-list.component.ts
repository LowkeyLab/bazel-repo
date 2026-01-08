import {
  ChangeDetectionStrategy,
  Component,
  signal,
  inject,
  OnInit,
} from '@angular/core';
import { Router } from '@angular/router';
import { ContestService } from '../../services/contest.service';
import { CircleService } from '../../services/circle.service';
import type { Contest, ContestStatus } from '../../models/contest.model';
import { DatePipe } from '@angular/common';
import { forkJoin, map, switchMap, of } from 'rxjs';

@Component({
  selector: 'contest-list',
  imports: [DatePipe],
  templateUrl: './contest-list.component.html',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ContestListComponent implements OnInit {
  private readonly router = inject(Router);
  private readonly contestService = inject(ContestService);
  private readonly circleService = inject(CircleService);

  public readonly contests = signal<Contest[]>([]);
  public readonly loading = signal(false);
  protected readonly statusLabels: Record<ContestStatus, string> = {
    OPEN: 'Open',
    LOCKED: 'Locked (awaiting resolution)',
    CLOSED: 'Closed (paused)',
    RESOLVED: 'Resolved',
  };

  ngOnInit(): void {
    this.loadAllContests();
  }

  private loadAllContests(): void {
    this.loading.set(true);
    this.circleService
      .listUserCircles()
      .pipe(
        switchMap((circles) => {
          if (circles.length === 0) return of([]);
          const contestRequests = circles.map((c) =>
            this.circleService.getCircleContests(c.id),
          );
          return forkJoin(contestRequests).pipe(
            map((results) => results.flat()),
          );
        }),
      )
      .subscribe({
        next: (allContests) => {
          allContests.sort(
            (a, b) =>
              new Date(b.created_at).getTime() -
              new Date(a.created_at).getTime(),
          );
          this.contests.set(allContests);
          this.loading.set(false);
        },
        error: (err) => {
          console.error('Failed to load contests:', err);
          this.loading.set(false);
        },
      });
  }

  viewContest(contest: Contest): void {
    this.router.navigate([
      '/circles',
      contest.circle_id,
      'contests',
      contest.id,
    ]);
  }

  public getTotalClout(contest: Contest): number {
    return contest.predictions.reduce((sum, pred) => sum + pred.clout, 0);
  }

  public refreshContests(): void {
    this.loadAllContests();
  }
}
