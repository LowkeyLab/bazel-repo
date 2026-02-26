import { ChangeDetectionStrategy, Component, inject, OnInit, signal } from '@angular/core';
import { RouterLink } from '@angular/router';
import { GetServersGQL, GetServersQuery } from '../generated/graphql';

type ServerEdge = NonNullable<GetServersQuery['servers']['edges']>[number];

@Component({
  selector: 'app-server-list',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [RouterLink],
  template: `
    <div class="p-4">
      <h1 class="text-2xl font-bold mb-4">Servers</h1>

      @if (loading() && edges().length === 0) {
        <span class="loading loading-spinner loading-md"></span>
      }

      @if (error()) {
        <div class="alert alert-error">{{ error() }}</div>
      }

      <ul class="menu bg-base-200 rounded-box w-full max-w-xl">
        @for (edge of edges(); track edge.node.id) {
          <li data-testid="server-row">
            <a [routerLink]="['/servers', edge.node.serverId, 'names']">
              Server {{ edge.node.serverId }}
            </a>
          </li>
        }
      </ul>

      @if (hasNextPage()) {
        <button
          class="btn btn-outline mt-4"
          data-testid="load-more"
          [disabled]="loading()"
          (click)="loadMore()"
        >
          @if (loading()) {
            <span class="loading loading-spinner loading-sm"></span>
          }
          Load more
        </button>
      }
    </div>
  `,
})
export class ServerListComponent implements OnInit {
  private readonly getServersGQL = inject(GetServersGQL);
  private queryRef?: ReturnType<GetServersGQL['watch']>;

  protected readonly edges = signal<ServerEdge[]>([]);
  protected readonly hasNextPage = signal(false);
  protected readonly endCursor = signal<string | null>(null);
  protected readonly loading = signal(true);
  protected readonly error = signal<string | null>(null);

  private static readonly PAGE_SIZE = 20;

  ngOnInit(): void {
    this.queryRef = this.getServersGQL.watch(
      { first: ServerListComponent.PAGE_SIZE },
      { fetchPolicy: 'cache-and-network' },
    );

    this.queryRef.valueChanges.subscribe({
      next: ({ data, loading }) => {
        this.edges.set(data.servers.edges);
        this.hasNextPage.set(data.servers.pageInfo.hasNextPage);
        this.endCursor.set(data.servers.pageInfo.endCursor ?? null);
        this.loading.set(loading);
      },
      error: (err) => {
        this.error.set(err.message);
        this.loading.set(false);
      },
    });
  }

  protected loadMore(): void {
    if (!this.queryRef || !this.endCursor()) return;
    this.loading.set(true);

    this.queryRef.fetchMore({
      variables: {
        first: ServerListComponent.PAGE_SIZE,
        after: this.endCursor(),
      },
    });
  }
}
