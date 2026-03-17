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
import { GetServersGQL, GetServersQuery } from '../../generated/graphql';

type ServerEdge = NonNullable<GetServersQuery['servers']['edges']>[number];

@Component({
  selector: 'app-server-list',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [RouterLink],
  template: `
    <div class="p-4">
      <div class="flex items-center justify-between mb-4">
        <h1 class="text-2xl font-bold">Servers</h1>
        <a routerLink="/servers/new" class="btn btn-primary" data-testid="add-server-btn">
          Add Server
        </a>
      </div>

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
              {{ edge.node.displayName }} ({{ edge.node.serverId }})
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
  private readonly destroyRef = inject(DestroyRef);
  private queryRef?: ReturnType<GetServersGQL['watch']>;

  protected readonly edges = signal<ServerEdge[]>([]);
  protected readonly hasNextPage = signal(false);
  private readonly endCursor = signal<string | null>(null);
  protected readonly loading = signal(true);
  protected readonly error = signal<string | null>(null);

  private static readonly PAGE_SIZE = 20;

  ngOnInit(): void {
    this.queryRef = this.getServersGQL.watch({
      variables: { first: ServerListComponent.PAGE_SIZE },
      fetchPolicy: 'cache-and-network',
    });

    this.queryRef.valueChanges
      .pipe(takeUntilDestroyed(this.destroyRef))
      .subscribe({
        next: ({ data, loading }) => {
          if (data?.servers?.edges) {
            this.edges.set(data.servers.edges as ServerEdge[]);
          }
          this.hasNextPage.set(data?.servers?.pageInfo?.hasNextPage ?? false);
          this.endCursor.set(data?.servers?.pageInfo?.endCursor ?? null);
          this.loading.set(loading);
        },
        error: (err: Error) => {
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
