import { describe, it, expect, vi, afterEach } from 'vitest';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideRouter, Router } from '@angular/router';
import {
  ApolloTestingController,
  ApolloTestingModule,
} from 'apollo-angular/testing';
import { AddServerComponent } from './add-server.component';
import { CreateServerDocument } from '../../generated/graphql';

describe('AddServerComponent', () => {
  let fixture: ComponentFixture<AddServerComponent>;
  let component: AddServerComponent;
  let apolloController: ApolloTestingController;

  beforeEach(async () => {
    await TestBed.configureTestingModule({
      imports: [AddServerComponent, ApolloTestingModule],
      providers: [provideRouter([])],
    }).compileComponents();

    fixture = TestBed.createComponent(AddServerComponent);
    component = fixture.componentInstance;
    apolloController = TestBed.inject(ApolloTestingController);
  });

  afterEach(() => {
    apolloController.verify();
  });

  it('should render form with inputs and disabled submit button', () => {
    fixture.detectChanges();

    const form = fixture.nativeElement.querySelector(
      '[data-testid="add-server-form"]',
    );
    expect(form).toBeTruthy();

    const serverIdInput = fixture.nativeElement.querySelector(
      '[data-testid="server-id-input"]',
    );
    expect(serverIdInput).toBeTruthy();

    const displayNameInput = fixture.nativeElement.querySelector(
      '[data-testid="display-name-input"]',
    );
    expect(displayNameInput).toBeTruthy();

    const submitButton: HTMLButtonElement = fixture.nativeElement.querySelector(
      '[data-testid="submit-server"]',
    );
    expect(submitButton).toBeTruthy();
    expect(submitButton.disabled).toBe(true);
  });

  it('should enable submit button when both fields filled', () => {
    component['serverId'].set('123456');
    component['displayName'].set('Test Server');
    fixture.detectChanges();

    const submitButton: HTMLButtonElement = fixture.nativeElement.querySelector(
      '[data-testid="submit-server"]',
    );
    expect(submitButton.disabled).toBe(false);
  });

  it('should navigate on successful mutation', async () => {
    const router = TestBed.inject(Router);
    vi.spyOn(router, 'navigate');

    component['serverId'].set('123456');
    component['displayName'].set('Test Server');
    fixture.detectChanges();

    const form: HTMLFormElement = fixture.nativeElement.querySelector(
      '[data-testid="add-server-form"]',
    );
    form.dispatchEvent(new Event('submit'));

    fixture.detectChanges();

    const op = apolloController.expectOne(CreateServerDocument);
    op.flush({
      data: {
        createServer: {
          clientMutationId: null,
          server: {
            id: 'relay-1',
            serverId: '123456',
            displayName: 'Test Server',
            createdAt: '2026-01-01T00:00:00Z',
            updatedAt: '2026-01-01T00:00:00Z',
          },
        },
      },
    });

    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
    fixture.detectChanges();

    expect(router.navigate).toHaveBeenCalledWith([
      '/servers',
      '123456',
      'names',
    ]);
  });

  it('should display error on mutation failure', async () => {
    component['serverId'].set('123456');
    component['displayName'].set('Test Server');
    fixture.detectChanges();

    const form: HTMLFormElement = fixture.nativeElement.querySelector(
      '[data-testid="add-server-form"]',
    );
    form.dispatchEvent(new Event('submit'));

    fixture.detectChanges();

    const op = apolloController.expectOne(CreateServerDocument);
    op.networkError(new Error('Server unavailable'));

    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
    fixture.detectChanges();

    const errorAlert = fixture.nativeElement.querySelector(
      '[data-testid="submit-error"]',
    );
    expect(errorAlert).toBeTruthy();
    expect(errorAlert.textContent).toContain('Server unavailable');
  });
});
