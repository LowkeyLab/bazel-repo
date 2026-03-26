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
        provideRouter([{ path: '**', component: BatchAddNamesComponent }]),
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
        '123456789012345678: Alice\n987654321098765432: Bob',
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

  describe('YAML parsing — not a map', () => {
    it('should show error when YAML is a plain string', () => {
      fixture.detectChanges();

      component['yamlInput'].set('just a string');
      fixture.detectChanges();

      component['onSubmit']();
      fixture.detectChanges();

      expect(component['error']()).toBe(
        'YAML must be a mapping of discord IDs to names',
      );
    });

    it('should show error when YAML is an array', () => {
      fixture.detectChanges();

      component['yamlInput'].set('- discordId: "123"\n  name: Alice');
      fixture.detectChanges();

      component['onSubmit']();
      fixture.detectChanges();

      expect(component['error']()).toBe(
        'YAML must be a mapping of discord IDs to names',
      );
    });
  });

  describe('YAML parsing — invalid discord ID key', () => {
    it('should show error when key is not a number', () => {
      fixture.detectChanges();

      component['yamlInput'].set('not-a-number: Alice');
      fixture.detectChanges();

      component['onSubmit']();
      fixture.detectChanges();

      expect(component['error']()).toBe(
        "Entry 'not-a-number': invalid Discord ID (must be a number)",
      );

      const errorDiv = fixture.nativeElement.querySelector(
        '[data-testid="batch-error"]',
      );
      expect(errorDiv).toBeTruthy();
      expect(errorDiv.textContent).toContain('invalid Discord ID');
    });
  });

  describe('YAML parsing — empty name value', () => {
    it('should show error when value is empty', () => {
      fixture.detectChanges();

      component['yamlInput'].set('123456789012345678:');
      fixture.detectChanges();

      component['onSubmit']();
      fixture.detectChanges();

      expect(component['error']()).toBe(
        "Entry '123456789012345678': missing or invalid name",
      );
    });
  });

  describe('YAML parsing — empty map', () => {
    it('should show error when YAML is an empty map', () => {
      fixture.detectChanges();

      component['yamlInput'].set('{}');
      fixture.detectChanges();

      component['onSubmit']();
      fixture.detectChanges();

      expect(component['error']()).toBe('No entries found in YAML');
    });
  });

  describe('mutation error', () => {
    it('should show error message when mutation fails', async () => {
      fixture.detectChanges();

      component['yamlInput'].set('123456789012345678: Alice');
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
