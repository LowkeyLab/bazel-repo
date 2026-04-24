import { inject, Injectable, InjectionToken, signal } from '@angular/core';

export interface StorageLike {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

export type RememberedChoices = Record<string, string>;

export const REMEMBERED_CHOICES_STORAGE_KEY = 'pinyin-hanzi.remembered-choices';

class InMemoryStorage implements StorageLike {
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

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function createStorage(): StorageLike {
  try {
    return globalThis.localStorage;
  } catch {
    return new InMemoryStorage();
  }
}

export const BROWSER_STORAGE = new InjectionToken<StorageLike>(
  'BROWSER_STORAGE',
  {
    providedIn: 'root',
    factory: createStorage,
  },
);

export function loadRememberedChoices(storage: StorageLike): RememberedChoices {
  const rawValue = storage.getItem(REMEMBERED_CHOICES_STORAGE_KEY);

  if (!rawValue) {
    return {};
  }

  try {
    const parsedValue: unknown = JSON.parse(rawValue);

    if (!isRecord(parsedValue)) {
      return {};
    }

    const rememberedChoices: RememberedChoices = {};

    for (const [key, value] of Object.entries(parsedValue)) {
      if (key.length > 0 && typeof value === 'string') {
        rememberedChoices[key] = value;
      }
    }

    return rememberedChoices;
  } catch {
    return {};
  }
}

export function saveRememberedChoices(
  storage: StorageLike,
  rememberedChoices: RememberedChoices,
): void {
  if (Object.keys(rememberedChoices).length === 0) {
    storage.removeItem(REMEMBERED_CHOICES_STORAGE_KEY);
    return;
  }

  storage.setItem(
    REMEMBERED_CHOICES_STORAGE_KEY,
    JSON.stringify(rememberedChoices),
  );
}

@Injectable({ providedIn: 'root' })
export class RememberedChoicesStore {
  private readonly storage = inject(BROWSER_STORAGE);

  readonly choices = signal<RememberedChoices>(
    loadRememberedChoices(this.storage),
  );

  rememberChoice(key: string, hanzi: string): void {
    const nextChoices = {
      ...this.choices(),
      [key]: hanzi,
    };

    saveRememberedChoices(this.storage, nextChoices);
    this.choices.set(nextChoices);
  }

  clear(): void {
    saveRememberedChoices(this.storage, {});
    this.choices.set({});
  }
}
