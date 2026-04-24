import {
  loadRememberedChoices,
  REMEMBERED_CHOICES_STORAGE_KEY,
  saveRememberedChoices,
} from './remembered-choices.store';
import type { StorageLike } from './remembered-choices.store';

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

describe('remembered choice persistence', () => {
  it('round-trips remembered choices through storage', () => {
    const storage = new FakeStorage();

    saveRememberedChoices(storage, { ma: '妈', shi: '事' });

    expect(loadRememberedChoices(storage)).toEqual({ ma: '妈', shi: '事' });
  });

  it('treats invalid stored json as empty state', () => {
    const storage = new FakeStorage();

    storage.setItem(REMEMBERED_CHOICES_STORAGE_KEY, '{not-json');

    expect(loadRememberedChoices(storage)).toEqual({});
  });
});
