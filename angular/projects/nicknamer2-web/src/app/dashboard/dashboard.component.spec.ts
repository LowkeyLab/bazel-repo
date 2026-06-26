import { ComponentFixture, TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { RouterLink, provideRouter } from '@angular/router';
import {
  ApolloTestingController,
  ApolloTestingModule,
} from 'apollo-angular/testing';
import { DashboardComponent } from './dashboard.component';
import { GetDashboardDocument } from '../../generated/graphql';

describe('DashboardComponent', () => {
  let fixture: ComponentFixture<DashboardComponent>;
  let component: DashboardComponent;
  let apolloController: ApolloTestingController;

  beforeEach(async () => {
    await TestBed.configureTestingModule({
      imports: [DashboardComponent, ApolloTestingModule],
      providers: [provideRouter([])],
    }).compileComponents();

    fixture = TestBed.createComponent(DashboardComponent);
    component = fixture.componentInstance;
    apolloController = TestBed.inject(ApolloTestingController);
  });

  afterEach(() => {
    apolloController.verify();
  });

  it('should display stats and server cards after loading', () => {
    fixture.detectChanges();

    const op = apolloController.expectOne(GetDashboardDocument);

    op.flush({
      data: {
        servers: {
          totalCount: 5,
          edges: [
            {
              node: {
                id: 'relay-1',
                serverId: '111',
                displayName: 'Primary Server',
              },
            },
            {
              node: {
                id: 'relay-2',
                serverId: '222',
                displayName: 'Backup Server',
              },
            },
          ],
        },
      },
    });

    fixture.detectChanges();

    const serverCount = fixture.nativeElement.querySelector(
      '[data-testid="server-count"]',
    );
    expect(serverCount).toBeTruthy();
    expect(serverCount.textContent).toContain('5');

    const cards = fixture.nativeElement.querySelectorAll(
      '[data-testid="server-card"]',
    );
    expect(cards.length).toBe(2);
    expect(cards[0].textContent).toContain('Primary Server');
    expect(cards[0].textContent).toContain('111');
    expect(cards[1].textContent).toContain('Backup Server');
    expect(cards[1].textContent).toContain('222');
  });

  it('should show loading skeleton initially', () => {
    fixture.detectChanges();

    const skeleton = fixture.nativeElement.querySelector(
      '[data-testid="loading-skeleton"]',
    );
    expect(skeleton).toBeTruthy();

    const op = apolloController.expectOne(GetDashboardDocument);
    op.flush({
      data: {
        servers: {
          totalCount: 0,
          edges: [],
        },
      },
    });
  });

  it('should show error message on query failure', () => {
    fixture.detectChanges();

    const op = apolloController.expectOne(GetDashboardDocument);
    op.graphqlErrors([{ message: 'Something went wrong' }]);

    fixture.detectChanges();

    const alert = fixture.nativeElement.querySelector(
      '[data-testid="error-alert"]',
    );
    expect(alert).toBeTruthy();
    expect(alert.textContent).toContain('Something went wrong');
  });

  it('should send correct variables', () => {
    fixture.detectChanges();

    const op = apolloController.expectOne(GetDashboardDocument);
    expect(op.operation.variables['first']).toBe(12);

    op.flush({
      data: {
        servers: {
          totalCount: 0,
          edges: [],
        },
      },
    });
  });

  it('should show "View all servers" link when servers exist', () => {
    fixture.detectChanges();

    const op = apolloController.expectOne(GetDashboardDocument);
    op.flush({
      data: {
        servers: {
          totalCount: 1,
          edges: [
            {
              node: {
                id: 'relay-1',
                serverId: '111',
                displayName: 'Primary Server',
              },
            },
          ],
        },
      },
    });

    fixture.detectChanges();

    const link = fixture.nativeElement.querySelector(
      '[data-testid="view-all-servers"]',
    );
    expect(link).toBeTruthy();
  });

  it('should hide "View all servers" link when no servers', () => {
    fixture.detectChanges();

    const op = apolloController.expectOne(GetDashboardDocument);
    op.flush({
      data: {
        servers: {
          totalCount: 0,
          edges: [],
        },
      },
    });

    fixture.detectChanges();

    const link = fixture.nativeElement.querySelector(
      '[data-testid="view-all-servers"]',
    );
    expect(link).toBeFalsy();
  });

  it('should show "Add Server" button linking to /servers/new', () => {
    fixture.detectChanges();

    const op = apolloController.expectOne(GetDashboardDocument);
    op.flush({
      data: {
        servers: {
          totalCount: 0,
          edges: [],
        },
      },
    });

    fixture.detectChanges();

    const linkDe = fixture.debugElement.query(
      By.css('[data-testid="add-server-btn"]'),
    );
    expect(linkDe).toBeTruthy();
    const routerLink = linkDe.injector.get(RouterLink);
    expect(routerLink.urlTree?.toString()).toBe('/servers/new');
  });
});
