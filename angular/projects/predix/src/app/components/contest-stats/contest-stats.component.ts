import { ChangeDetectionStrategy, Component, input } from '@angular/core';
import { ContestStatComponent } from '../contest-stat/contest-stat.component';

export interface StatItem {
  title: string;
  value: number;
}

@Component({
  selector: 'contest-stats',
  standalone: true,
  imports: [ContestStatComponent],
  templateUrl: './contest-stats.component.html',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: {
    class: 'stats stats-vertical lg:stats-horizontal',
  },
})
export class ContestStatsComponent {
  public readonly stats = input.required<StatItem[]>();
}
