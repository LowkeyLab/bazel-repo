import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
} from '@angular/core';
import { RouterLink } from '@angular/router';

import { AuthService } from '../../services/auth.service';

@Component({
  selector: 'app-home',
  imports: [RouterLink],
  template: `
    <div class="bg-base-200">
      <section class="hero py-16 px-4">
        <div class="max-w-6xl mx-auto grid gap-12 md:grid-cols-2 items-center">
          <div class="space-y-6">
            <div class="badge badge-primary badge-lg">Private Circle Arena</div>
            <h1 class="text-4xl md:text-5xl font-bold leading-tight">
              Prediction battles built for your inner circle
            </h1>
            <p class="text-lg text-secondary">
              Predix lets friends, roommates, and coworkers wager Clout on the
              micro-moments you actually care about—inside invite-only Circles.
            </p>
            <div class="flex flex-wrap gap-3">
              <a class="btn btn-primary" [routerLink]="primaryCta()">
                {{ primaryLabel() }}
              </a>
              <a class="btn btn-outline" [routerLink]="secondaryCta()">
                {{ secondaryLabel() }}
              </a>
            </div>
            <div class="flex flex-wrap gap-2 text-sm text-secondary">
              <span class="badge badge-outline">Invite-only Circles</span>
              <span class="badge badge-outline">Clout pools</span>
              <span class="badge badge-outline">Creator-led resolution</span>
              <span class="badge badge-outline">Live trash talk</span>
            </div>
          </div>
          <div class="grid gap-4">
            <div class="card bg-base-100 shadow-xl">
              <div class="card-body space-y-3">
                <h2 class="card-title">How it works</h2>
                <ul
                  class="list-disc list-inside space-y-2 text-base-content/80"
                >
                  <li>Create a Circle and share the invite code.</li>
                  <li>
                    Spin up friendly predictions with dynamic Clout pools.
                  </li>
                  <li>Resolve outcomes with the Circle creator or a vote.</li>
                  <li>Climb the local leaderboard and earn bragging rights.</li>
                </ul>
              </div>
            </div>
            <div class="grid md:grid-cols-2 gap-4">
              <div class="card bg-base-100 shadow">
                <div class="card-body">
                  <p class="text-sm text-secondary">For friend groups</p>
                  <h3 class="text-xl font-semibold">Sunday Football Crew</h3>
                  <p class="text-sm text-secondary">
                    Who calls the plays? Predict the outcomes that actually
                    matter to your crew.
                  </p>
                </div>
              </div>
              <div class="card bg-base-100 shadow">
                <div class="card-body">
                  <p class="text-sm text-secondary">For roommates</p>
                  <h3 class="text-xl font-semibold">Dish Duty Showdown</h3>
                  <p class="text-sm text-secondary">
                    Bet Clout on chores, dares, and daily moments with stakes
                    everyone can laugh about.
                  </p>
                </div>
              </div>
            </div>
          </div>
        </div>
      </section>

      <section class="py-12 px-4 bg-base-100 border-t border-base-300">
        <div class="max-w-6xl mx-auto grid gap-6 md:grid-cols-3">
          @for (highlight of highlights; track highlight.title) {
            <div class="card bg-base-200 shadow-sm">
              <div class="card-body space-y-2">
                <div class="text-3xl">{{ highlight.emoji }}</div>
                <h3 class="text-xl font-semibold">{{ highlight.title }}</h3>
                <p class="text-sm text-secondary">{{ highlight.copy }}</p>
              </div>
            </div>
          }
        </div>
      </section>
    </div>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class HomeComponent {
  private readonly auth = inject(AuthService);

  protected readonly primaryCta = computed(() =>
    this.auth.isAuthenticated() ? '/circles' : '/login',
  );
  protected readonly secondaryCta = computed(() =>
    this.auth.isAuthenticated() ? '/contests' : '/login',
  );
  protected readonly primaryLabel = computed(() =>
    this.auth.isAuthenticated() ? 'Open your circles' : 'Sign in to start',
  );
  protected readonly secondaryLabel = computed(() =>
    this.auth.isAuthenticated() ? 'Browse contests' : 'Preview contests',
  );

  protected readonly highlights = [
    {
      title: 'Private arenas',
      copy: 'Invite-only Circles keep wagers, balances, and leaderboards contained to your crew.',
      emoji: '🔒',
    },
    {
      title: 'Dynamic Clout pools',
      copy: 'Underdogs earn more when the odds are stacked—every prediction balances the pot.',
      emoji: '💥',
    },
    {
      title: 'Social energy',
      copy: 'Trash talk ticker, weekly recap cards, and challenges keep friends coming back.',
      emoji: '🎉',
    },
  ];
}
