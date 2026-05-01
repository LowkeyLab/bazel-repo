import { TestBed } from '@angular/core/testing';

import {
  LOCAL_DOCUMENT_STORAGE,
  LocalDocumentStoreService,
} from './local-document-store.service';

describe('LocalDocumentStoreService', () => {
  let storage: MapStorage;

  beforeEach(() => {
    storage = new MapStorage();
    TestBed.configureTestingModule({
      providers: [{ provide: LOCAL_DOCUMENT_STORAGE, useValue: storage }],
    });
  });

  it('saves and lists composer documents', () => {
    const service = TestBed.inject(LocalDocumentStoreService);

    service.saveDocument({
      id: 'doc-1',
      title: 'Lesson 1',
      updatedAtIso: '2026-04-30T12:00:00.000Z',
      tokens: [
        {
          id: 'token-1',
          sourcePinyin: 'beijing',
          hanzi: '北京',
          displayPinyin: 'Běijīng',
        },
      ],
    });

    expect(service.listDocuments()).toEqual([
      {
        id: 'doc-1',
        title: 'Lesson 1',
        updatedAtIso: '2026-04-30T12:00:00.000Z',
        tokens: [
          {
            id: 'token-1',
            sourcePinyin: 'beijing',
            hanzi: '北京',
            displayPinyin: 'Běijīng',
          },
        ],
      },
    ]);
  });
});

class MapStorage implements Storage {
  private readonly values = new Map<string, string>();

  get length(): number {
    return this.values.size;
  }

  clear(): void {
    this.values.clear();
  }

  getItem(key: string): string | null {
    return this.values.get(key) ?? null;
  }

  key(index: number): string | null {
    return [...this.values.keys()][index] ?? null;
  }

  removeItem(key: string): void {
    this.values.delete(key);
  }

  setItem(key: string, value: string): void {
    this.values.set(key, value);
  }
}
