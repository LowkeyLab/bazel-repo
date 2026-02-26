import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  inject,
  input,
  OnInit,
  signal,
} from '@angular/core';
import { takeUntilDestroyed } from '@angular/core/rxjs-interop';
import { DatePipe } from '@angular/common';
import { RouterLink } from '@angular/router';
import { GetServerNamesGQL, GetServerNamesQuery } from '../generated/graphql';

type NameEdge = NonNullable<
  GetServerNamesQuery['server']['names']['edges']
>[number];

@Component({
  selector: 'app-server-names',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [RouterLink, DatePipe],
  template: `
    <div class="p-4">
      <a routerLink="/servers" class="link link-hover mb-4 inline-block">
        &larr; Back to servers
      </a>

      <h1 class="text-2xl font-bold mb-4">
        Names for Server {{ serverId() }}
      </h1>

      @if (loading() && edges().length === 0) {
        <span class="loading loading-spinner loading-md"></span>
      }

      @if (error()) {
        <div class="alert alert-error">{{ error() }}</div>
      }

      @if (edges().length > 0) {
        <div class="overflow-x-auto">
          <table class="table">
            <thead>
              <tr>
                <th>Name</th>
                <th>Created At</th>
                <th>Updated At</th>
              </tr>
            </thead>
            <tbody>
              @for (edge of edges(); track edge.node.id) {
                <tr data-testid="name-row">
                  <td>{{ edge.node.name }}</td>
                  <td>{{ edge.node.createdAt | date: 'medium' }}</td>
                  <td>{{ edge.node.updatedAt | date: 'medium' }}</td>
                </tr>
              }
            </tbody>
          </table>
        </div>
      } @else if (!loading()) {
        <p class="text-base-content/60">No names found for this server.</p>
      }

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
export class ServerNamesComponent implements OnInit {
  readonly serverId = input.required<string>();

  private readonly getServerNamesGQL = inject(GetServerNamesGQL);
  private readonly destroyRef = inject(DestroyRef);
  private queryRef?: ReturnType<GetServerNamesGQL['watch']>;

  protected readonly edges = signal<NameEdge[]>([]);
  protected readonly hasNextPage = signal(false);
  protected readonly endCursor = signal<string | null>(null);
  protected readonly loading = signal(true);
  protected readonly error = signal<string | null>(null);

  private static readonly PAGE_SIZE = 20;

  ngOnInit(): void {
    this.queryRef = this.getServerNamesGQL.watch(
      { id: this.serverId(), first: ServerNamesComponent.PAGE_SIZE },
      { fetchPolicy: 'cache-and-network' },
    );

    this.queryRef.valueChanges
      .pipe(takeUntilDestroyed(this.destroyRef))
      .subscribe({
      next: ({ data, loading }) => {
        this.edges.set(data.server.names.edges);
        this.hasNextPage.set(data.server.names.pageInfo.hasNextPage);
        this.endCursor.set(data.server.names.pageInfo.endCursor ?? null);
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
        id: this.serverId(),
        first: ServerNamesComponent.PAGE_SIZE,
        after: this.endCursor(),
      },
    });
  }
}
