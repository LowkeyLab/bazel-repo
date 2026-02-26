import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { ApolloTestingController, ApolloTestingModule } from 'apollo-angular/testing';
import { ServerListComponent } from './server-list.component';
import { GetServersDocument } from '../generated/graphql';

describe('ServerListComponent', () => {
  let fixture: ComponentFixture<ServerListComponent>;
  let component: ServerListComponent;
  let apolloController: ApolloTestingController;

  beforeEach(async () => {
    await TestBed.configureTestingModule({
      imports: [ServerListComponent, ApolloTestingModule],
      providers: [provideRouter([])],
    }).compileComponents();

    fixture = TestBed.createComponent(ServerListComponent);
    component = fixture.componentInstance;
    apolloController = TestBed.inject(ApolloTestingController);
  });

  afterEach(() => {
    apolloController.verify();
  });

  it('should display servers after loading', () => {
    fixture.detectChanges();

    const op = apolloController.expectOne(GetServersDocument);
    expect(op.operation.variables['first']).toBe(20);

    op.flush({
      data: {
        servers: {
          edges: [
            { cursor: 'c1', node: { id: 'relay-1', serverId: '111' } },
            { cursor: 'c2', node: { id: 'relay-2', serverId: '222' } },
          ],
          pageInfo: { hasNextPage: false, endCursor: 'c2' },
        },
      },
    });

    fixture.detectChanges();

    const rows = fixture.nativeElement.querySelectorAll('[data-testid="server-row"]');
    expect(rows.length).toBe(2);
    expect(rows[0].textContent).toContain('111');
    expect(rows[1].textContent).toContain('222');
  });

  it('should show load more button when hasNextPage is true', () => {
    fixture.detectChanges();

    const op = apolloController.expectOne(GetServersDocument);
    op.flush({
      data: {
        servers: {
          edges: [
            { cursor: 'c1', node: { id: 'relay-1', serverId: '111' } },
          ],
          pageInfo: { hasNextPage: true, endCursor: 'c1' },
        },
      },
    });

    fixture.detectChanges();

    const btn = fixture.nativeElement.querySelector('[data-testid="load-more"]');
    expect(btn).toBeTruthy();
  });

  it('should hide load more button when hasNextPage is false', () => {
    fixture.detectChanges();

    const op = apolloController.expectOne(GetServersDocument);
    op.flush({
      data: {
        servers: {
          edges: [
            { cursor: 'c1', node: { id: 'relay-1', serverId: '111' } },
          ],
          pageInfo: { hasNextPage: false, endCursor: 'c1' },
        },
      },
    });

    fixture.detectChanges();

    const btn = fixture.nativeElement.querySelector('[data-testid="load-more"]');
    expect(btn).toBeFalsy();
  });
});
