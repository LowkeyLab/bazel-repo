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
  private readonly auth = inject(AuthService);

  protected readonly primaryCta = computed(() =>
    this.auth.isAuthenticated() ? '/circles' : '/register',
  );
  protected readonly secondaryCta = computed(() =>
    this.auth.isAuthenticated() ? '/contests' : '/login',
  );
  protected readonly primaryLabel = computed(() =>
    this.auth.isAuthenticated() ? 'Open your circles' : 'Create your account',
  );
  protected readonly secondaryLabel = computed(() =>
    this.auth.isAuthenticated() ? 'Browse contests' : 'Sign in to preview',
  );

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
