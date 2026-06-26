import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  inject,
  OnInit,
  signal,
} from '@angular/core';
import { takeUntilDestroyed } from '@angular/core/rxjs-interop';
import { RouterLink } from '@angular/router';
import { GetDashboardGQL, GetDashboardQuery } from '../../generated/graphql';

type ServerEdge = GetDashboardQuery['servers']['edges'][number];

@Component({
  selector: 'app-dashboard',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [RouterLink],
  template: `
    <div class="p-4">
      <div class="flex items-center justify-between mb-6">
        <h1 class="text-3xl font-bold">Nicknamer2</h1>
        <a
          routerLink="/servers/new"
          class="btn btn-primary"
          data-testid="add-server-btn"
        >
          Add Server
        </a>
      </div>

      @if (loading()) {
        <div data-testid="loading-skeleton" class="flex gap-4 mb-6">
          <div class="skeleton h-24 w-40"></div>
          <div class="skeleton h-24 w-40"></div>
        </div>
        <div class="skeleton h-48 w-full"></div>
      } @else if (error()) {
        <div data-testid="error-alert" class="alert alert-error mb-4">
          {{ error() }}
        </div>
      } @else {
        <div class="stats shadow mb-6">
          <div class="stat">
            <div class="stat-title">Servers</div>
            <div data-testid="server-count" class="stat-value">
              {{ totalServers() }}
            </div>
          </div>
        </div>

        <h2 class="text-xl font-semibold mb-4">Servers</h2>
        <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          @for (edge of edges(); track edge.node.id) {
            <a
              data-testid="server-card"
              [routerLink]="['/servers', edge.node.serverId, 'names']"
              class="card bg-base-200 hover:bg-base-300 transition-colors cursor-pointer"
            >
              <div class="card-body">
                <h3 class="card-title">{{ edge.node.displayName }}</h3>
                <p class="text-sm opacity-70">{{ edge.node.serverId }}</p>
              </div>
            </a>
          }
        </div>

        @if (edges().length > 0) {
          <div class="mt-4">
            <a
              data-testid="view-all-servers"
              routerLink="/servers"
              class="btn btn-outline"
              >View all servers</a
            >
          </div>
        }
      }
    </div>
  `,
})
export class DashboardComponent implements OnInit {
  private readonly getDashboardGQL = inject(GetDashboardGQL);
  private readonly destroyRef = inject(DestroyRef);

  protected readonly edges = signal<ServerEdge[]>([]);
  protected readonly totalServers = signal(0);
  protected readonly loading = signal(true);
  protected readonly error = signal<string | null>(null);

  private static readonly PAGE_SIZE = 12;

  ngOnInit(): void {
    this.getDashboardGQL
      .watch({
        variables: { first: DashboardComponent.PAGE_SIZE },
        fetchPolicy: 'cache-and-network',
        errorPolicy: 'all',
      })
      .valueChanges.pipe(takeUntilDestroyed(this.destroyRef))
      .subscribe(({ data, loading, error }) => {
        this.loading.set(loading);
        if (error) {
          this.loading.set(false);
          this.error.set(error.message);
          return;
        }
        if (data?.servers) {
          this.edges.set(data.servers.edges as ServerEdge[]);
          this.totalServers.set(data.servers.totalCount ?? 0);
        }
      });
  }
}
