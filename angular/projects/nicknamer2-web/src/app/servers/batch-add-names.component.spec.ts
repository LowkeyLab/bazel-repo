import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import {
  ApolloTestingController,
  ApolloTestingModule,
} from 'apollo-angular/testing';
import { BatchAddNamesComponent } from './batch-add-names.component';
import { CreateNamesDocument } from '../../generated/graphql';

describe('BatchAddNamesComponent', () => {
  let fixture: ComponentFixture<BatchAddNamesComponent>;
  let component: BatchAddNamesComponent;
  let apolloController: ApolloTestingController;

  beforeEach(async () => {
    await TestBed.configureTestingModule({
      imports: [BatchAddNamesComponent, ApolloTestingModule],
      providers: [
        provideRouter([
          { path: '**', component: BatchAddNamesComponent },
        ]),
      ],
    }).compileComponents();

    fixture = TestBed.createComponent(BatchAddNamesComponent);
    component = fixture.componentInstance;
    fixture.componentRef.setInput('serverId', '99999');
    apolloController = TestBed.inject(ApolloTestingController);
  });

  afterEach(() => {
    apolloController.verify();
  });

  describe('YAML parsing — valid input', () => {
    it('should submit valid YAML and show success count', async () => {
      fixture.detectChanges();

      component['yamlInput'].set(
        '- discordId: "123456789012345678"\n  name: Alice\n- discordId: "987654321098765432"\n  name: Bob',
      );
      fixture.detectChanges();

      const form: HTMLFormElement = fixture.nativeElement.querySelector(
        '[data-testid="batch-form"]',
      );
      form.dispatchEvent(new Event('submit'));
      fixture.detectChanges();

      const op = apolloController.expectOne(CreateNamesDocument);
      const input = op.operation.variables['input'];
      expect(input['discordServerId']).toBe('99999');
      expect(input['names']).toEqual([
        { discordId: '123456789012345678', name: 'Alice' },
        { discordId: '987654321098765432', name: 'Bob' },
      ]);

      op.flush({
        data: {
          createNames: {
            clientMutationId: null,
            names: [
              {
                id: 'n1',
                name: 'Alice',
                createdAt: '2026-01-01T00:00:00Z',
                updatedAt: '2026-01-01T00:00:00Z',
              },
              {
                id: 'n2',
                name: 'Bob',
                createdAt: '2026-01-01T00:00:00Z',
                updatedAt: '2026-01-01T00:00:00Z',
              },
            ],
          },
        },
      });

      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
      fixture.detectChanges();

      expect(component['successCount']()).toBe(2);
      expect(component['error']()).toBeNull();

      const successDiv = fixture.nativeElement.querySelector(
        '[data-testid="batch-success"]',
      );
      expect(successDiv).toBeTruthy();
      expect(successDiv.textContent).toContain('2');
    });
  });

  describe('YAML parsing — not an array', () => {
    it('should show error when YAML is a plain string', () => {
      fixture.detectChanges();

      component['yamlInput'].set('just a string');
      fixture.detectChanges();

      component['onSubmit']();
      fixture.detectChanges();

      expect(component['error']()).toBe('YAML must be a list of entries');
    });

    it('should show error when YAML is an object', () => {
      fixture.detectChanges();

      component['yamlInput'].set('discordId: "123"\nname: Alice');
      fixture.detectChanges();

      component['onSubmit']();
      fixture.detectChanges();

      expect(component['error']()).toBe('YAML must be a list of entries');
    });
  });

  describe('YAML parsing — missing discordId', () => {
    it('should show error when entry is missing discordId', () => {
      fixture.detectChanges();

      component['yamlInput'].set('- name: Alice');
      fixture.detectChanges();

      component['onSubmit']();
      fixture.detectChanges();

      expect(component['error']()).toBe('Entry 1: missing or invalid discordId');

      const errorDiv = fixture.nativeElement.querySelector(
        '[data-testid="batch-error"]',
      );
      expect(errorDiv).toBeTruthy();
      expect(errorDiv.textContent).toContain(
        'Entry 1: missing or invalid discordId',
      );
    });
  });

  describe('YAML parsing — missing name', () => {
    it('should show error when entry is missing name', () => {
      fixture.detectChanges();

      component['yamlInput'].set('- discordId: "123456789012345678"');
      fixture.detectChanges();

      component['onSubmit']();
      fixture.detectChanges();

      expect(component['error']()).toBe('Entry 1: missing or invalid name');
    });
  });

  describe('YAML parsing — numeric discordId rejected', () => {
    it('should show precision loss error when discordId is an unquoted number', () => {
      fixture.detectChanges();

      // Unquoted large number — js-yaml parses this as a JS number
      component['yamlInput'].set(
        '- discordId: 123456789012345678\n  name: Alice',
      );
      fixture.detectChanges();

      component['onSubmit']();
      fixture.detectChanges();

      expect(component['error']()).toBe(
        'Entry 1: discordId must be a quoted string to avoid precision loss',
      );

      const errorDiv = fixture.nativeElement.querySelector(
        '[data-testid="batch-error"]',
      );
      expect(errorDiv).toBeTruthy();
      expect(errorDiv.textContent).toContain('precision loss');
    });
  });

  describe('YAML parsing — empty array', () => {
    it('should show error when YAML is an empty array', () => {
      fixture.detectChanges();

      component['yamlInput'].set('[]');
      fixture.detectChanges();

      component['onSubmit']();
      fixture.detectChanges();

      expect(component['error']()).toBe('No entries found in YAML');
    });
  });

  describe('mutation error', () => {
    it('should show error message when mutation fails', async () => {
      fixture.detectChanges();

      component['yamlInput'].set(
        '- discordId: "123456789012345678"\n  name: Alice',
      );
      fixture.detectChanges();

      component['onSubmit']();
      fixture.detectChanges();

      const op = apolloController.expectOne(CreateNamesDocument);
      op.networkError(new Error('Network failure'));

      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
      fixture.detectChanges();

      expect(component['error']()).not.toBeNull();
      expect(component['successCount']()).toBeNull();
      expect(component['submitting']()).toBe(false);
    });
  });
});
