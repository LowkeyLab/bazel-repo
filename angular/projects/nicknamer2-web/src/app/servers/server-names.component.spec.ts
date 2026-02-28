import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import {
  ApolloTestingController,
  ApolloTestingModule,
} from 'apollo-angular/testing';
import { ServerNamesComponent } from './server-names.component';
import { GetServerNamesDocument, CreateNameDocument } from '../../generated/graphql';

describe('ServerNamesComponent', () => {
  let fixture: ComponentFixture<ServerNamesComponent>;
  let component: ServerNamesComponent;
  let apolloController: ApolloTestingController;

  beforeEach(async () => {
    await TestBed.configureTestingModule({
      imports: [ServerNamesComponent, ApolloTestingModule],
      providers: [
        provideRouter([{ path: '**', component: ServerNamesComponent }]),
      ],
    }).compileComponents();

    fixture = TestBed.createComponent(ServerNamesComponent);
    component = fixture.componentInstance;
    fixture.componentRef.setInput('serverId', '12345');
    apolloController = TestBed.inject(ApolloTestingController);
  });

  afterEach(() => {
    apolloController.verify();
  });

  it('should display names in a table', () => {
    fixture.detectChanges();

    const op = apolloController.expectOne(GetServerNamesDocument);
    expect(op.operation.variables['id']).toBe('12345');
    expect(op.operation.variables['first']).toBe(20);

    op.flush({
      data: {
        server: {
          id: 'relay-1',
          serverId: '12345',
          names: {
            edges: [
              {
                cursor: 'c1',
                node: {
                  id: 'name-1',
                  name: 'Alice',
                  createdAt: '2025-01-01T00:00:00Z',
                  updatedAt: '2025-06-15T12:00:00Z',
                },
              },
              {
                cursor: 'c2',
                node: {
                  id: 'name-2',
                  name: 'Bob',
                  createdAt: '2025-02-01T00:00:00Z',
                  updatedAt: '2025-07-20T08:00:00Z',
                },
              },
            ],
            pageInfo: { hasNextPage: false, endCursor: 'c2' },
          },
        },
      },
    });

    fixture.detectChanges();

    const rows = fixture.nativeElement.querySelectorAll(
      '[data-testid="name-row"]',
    );
    expect(rows.length).toBe(2);
    expect(rows[0].textContent).toContain('Alice');
    expect(rows[1].textContent).toContain('Bob');
  });

  it('should show load more button when hasNextPage is true', () => {
    fixture.detectChanges();

    const op = apolloController.expectOne(GetServerNamesDocument);
    op.flush({
      data: {
        server: {
          id: 'relay-1',
          serverId: '12345',
          names: {
            edges: [
              {
                cursor: 'c1',
                node: {
                  id: 'name-1',
                  name: 'Alice',
                  createdAt: '2025-01-01T00:00:00Z',
                  updatedAt: '2025-06-15T12:00:00Z',
                },
              },
            ],
            pageInfo: { hasNextPage: true, endCursor: 'c1' },
          },
        },
      },
    });

    fixture.detectChanges();

    const btn = fixture.nativeElement.querySelector(
      '[data-testid="load-more"]',
    );
    expect(btn).toBeTruthy();
  });

  it('should submit the form and refetch names', async () => {
    fixture.detectChanges();

    // Flush initial query
    const initialOp = apolloController.expectOne(GetServerNamesDocument);
    initialOp.flush({
      data: {
        server: {
          id: 'relay-1',
          serverId: '12345',
          names: {
            edges: [],
            pageInfo: { hasNextPage: false, endCursor: null },
          },
        },
      },
    });
    fixture.detectChanges();

    // Fill in the form by directly updating the component signals
    component['discordId'].set('999');
    component['nickname'].set('NewNickname');
    fixture.detectChanges();

    // Submit the form by triggering ngSubmit on the form element
    const form: HTMLFormElement = fixture.nativeElement.querySelector('[data-testid="add-name-form"]');
    form.dispatchEvent(new Event('submit'));
    fixture.detectChanges();

    // Expect mutation
    const mutationOp = apolloController.expectOne(CreateNameDocument);
    expect(mutationOp.operation.variables['discordId']).toBe('999');
    expect(mutationOp.operation.variables['discordServerId']).toBe('12345');
    expect(mutationOp.operation.variables['name']).toBe('NewNickname');

    mutationOp.flush({
      data: {
        createName: {
          id: 'name-new',
          name: 'NewNickname',
          createdAt: '2026-01-01T00:00:00Z',
          updatedAt: '2026-01-01T00:00:00Z',
        },
      },
    });
    // Allow microtasks to run so the refetch triggered by the mutation's next callback can execute
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
    fixture.detectChanges();

    // Expect refetch
    const refetchOp = apolloController.expectOne(GetServerNamesDocument);
    refetchOp.flush({
      data: {
        server: {
          id: 'relay-1',
          serverId: '12345',
          names: {
            edges: [
              {
                cursor: 'c1',
                node: {
                  id: 'name-new',
                  name: 'NewNickname',
                  createdAt: '2026-01-01T00:00:00Z',
                  updatedAt: '2026-01-01T00:00:00Z',
                },
              },
            ],
            pageInfo: { hasNextPage: false, endCursor: 'c1' },
          },
        },
      },
    });
    fixture.detectChanges();

    const rows = fixture.nativeElement.querySelectorAll('[data-testid="name-row"]');
    expect(rows.length).toBe(1);
    expect(rows[0].textContent).toContain('NewNickname');
  });

  it('should display the server ID in the heading', () => {
    fixture.detectChanges();

    const op = apolloController.expectOne(GetServerNamesDocument);
    op.flush({
      data: {
        server: {
          id: 'relay-1',
          serverId: '12345',
          names: {
            edges: [],
            pageInfo: { hasNextPage: false, endCursor: null },
          },
        },
      },
    });

    fixture.detectChanges();

    const heading = fixture.nativeElement.querySelector('h1');
    expect(heading.textContent).toContain('12345');
  });
});
