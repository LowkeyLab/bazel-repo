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
      <h1 class="text-3xl font-bold mb-6">Nicknamer2</h1>

      @if (loading()) {
        <div class="flex gap-4 mb-6">
          <div class="skeleton h-24 w-40"></div>
          <div class="skeleton h-24 w-40"></div>
        </div>
        <div class="skeleton h-48 w-full"></div>
      } @else if (error()) {
        <div class="alert alert-error mb-4">{{ error() }}</div>
      } @else {
        <div class="stats shadow mb-6">
          <div class="stat">
            <div class="stat-title">Servers</div>
            <div class="stat-value">{{ totalServers() }}</div>
          </div>
        </div>

        <h2 class="text-xl font-semibold mb-4">Servers</h2>
        <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          @for (edge of edges(); track edge.node.id) {
            <a
              [routerLink]="['/servers', edge.node.serverId, 'names']"
              class="card bg-base-200 hover:bg-base-300 transition-colors cursor-pointer"
            >
              <div class="card-body">
                <h3 class="card-title">Server {{ edge.node.serverId }}</h3>
              </div>
            </a>
          }
        </div>

        @if (edges().length > 0) {
          <div class="mt-4">
            <a routerLink="/servers" class="btn btn-outline"
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
      .watch(
        { first: DashboardComponent.PAGE_SIZE },
        { fetchPolicy: 'cache-and-network' },
      )
      .valueChanges.pipe(takeUntilDestroyed(this.destroyRef))
      .subscribe({
        next: ({ data, loading }) => {
          this.loading.set(loading);
          if (data?.servers) {
            this.edges.set(data.servers.edges as ServerEdge[]);
            this.totalServers.set(data.servers.totalCount);
          }
        },
        error: (err: Error) => {
          this.loading.set(false);
          this.error.set(err.message);
        },
      });
  }
}
