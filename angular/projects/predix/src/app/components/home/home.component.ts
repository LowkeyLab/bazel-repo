import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
} from '@angular/core';
import { RouterLink } from '@angular/router';

import { AuthService } from '../../services/auth.service';

@Component({
  selector: 'home',
  imports: [RouterLink],
  templateUrl: './home.component.html',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class HomeComponent {
  protected readonly auth = inject(AuthService);

  protected readonly primaryLabel = computed(() => 'Start a circle');
  protected readonly secondaryLabel = computed(() => 'Browse contests');

  protected login(): void {
    this.auth.login();
  }

  protected readonly highlights = [
    {
      title: 'Friends-first',
      copy: 'Invite-only Circles keep Contests, Clout ledgers, and leaderboards for you and your friends.',
      emoji: '🔒',
    },
    {
      title: 'Clout stakes',
      copy: 'Members start with some Clout and stake it on Options—odds shift as predictions land.',
      emoji: '💎',
    },
    {
      title: 'Social energy',
      copy: 'Live ticks, payouts, and trash talk make predicting with friends more fun.',
      emoji: '🎉',
    },
  ];
}
