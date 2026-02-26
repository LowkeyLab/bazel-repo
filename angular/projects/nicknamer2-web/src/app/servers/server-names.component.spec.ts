import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { ApolloTestingController, ApolloTestingModule } from 'apollo-angular/testing';
import { ServerNamesComponent } from './server-names.component';
import { GetServerNamesDocument } from '../generated/graphql';

describe('ServerNamesComponent', () => {
  let fixture: ComponentFixture<ServerNamesComponent>;
  let component: ServerNamesComponent;
  let apolloController: ApolloTestingController;

  beforeEach(async () => {
    await TestBed.configureTestingModule({
      imports: [ServerNamesComponent, ApolloTestingModule],
      providers: [provideRouter([{ path: '**', component: ServerNamesComponent }])],
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

    const rows = fixture.nativeElement.querySelectorAll('[data-testid="name-row"]');
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

    const btn = fixture.nativeElement.querySelector('[data-testid="load-more"]');
    expect(btn).toBeTruthy();
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
