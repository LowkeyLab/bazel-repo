import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';

import { App } from './app';
import {
  BROWSER_STORAGE,
  loadRememberedChoices,
  type StorageLike,
} from './remembered-choices.store';

class FakeStorage implements StorageLike {
  private readonly values = new Map<string, string>();

  getItem(key: string): string | null {
    return this.values.get(key) ?? null;
  }

  setItem(key: string, value: string): void {
    this.values.set(key, value);
  }

  removeItem(key: string): void {
    this.values.delete(key);
  }
}

describe('App', () => {
  let storage: FakeStorage;

  beforeEach(async () => {
    storage = new FakeStorage();

    await TestBed.configureTestingModule({
      imports: [App],
      providers: [
        provideZonelessChangeDetection(),
        {
          provide: BROWSER_STORAGE,
          useValue: storage,
        },
      ],
    }).compileComponents();
  });

  it('creates the app', () => {
    const fixture = TestBed.createComponent(App);

    expect(fixture.componentInstance).toBeTruthy();
  });

  it('updates the rendered output and remembered storage when a choice is selected', () => {
    const fixture = TestBed.createComponent(App);
    fixture.detectChanges();

    const host = fixture.nativeElement as HTMLElement;
    const output = host.querySelector('[data-testid="plain-output"]');
    expect(output?.textContent?.trim()).toBe('你好吗');

    const motherButton = host.querySelector(
      '[data-testid="candidate-choice"][data-key="ma"][data-hanzi="妈"]',
    );
    motherButton?.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    fixture.detectChanges();

    expect(output?.textContent?.trim()).toBe('你好妈');
    expect(loadRememberedChoices(storage)).toEqual({ ma: '妈' });

    const clearButton = host.querySelector('[data-testid="clear-memory"]');
    clearButton?.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    fixture.detectChanges();

    expect(output?.textContent?.trim()).toBe('你好吗');
    expect(loadRememberedChoices(storage)).toEqual({});
  });
});
