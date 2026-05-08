import { TestBed } from '@angular/core/testing';

import {
  LOCAL_DOCUMENT_STORAGE,
  LocalDocumentStoreService,
} from './local-document-store.service';

const V1_STORAGE_KEY = 'pinyin-composer.documents.v1';
const V2_STORAGE_KEY = 'pinyin-composer.documents.v2';

describe('LocalDocumentStoreService', () => {
  let storage: MapStorage;

  beforeEach(() => {
    storage = new MapStorage();
    TestBed.configureTestingModule({
      providers: [{ provide: LOCAL_DOCUMENT_STORAGE, useValue: storage }],
    });
  });

  it('ignores old v1 token documents without deleting them', () => {
    const service = TestBed.inject(LocalDocumentStoreService);
    const v1Payload = JSON.stringify([
      {
        schemaVersion: 1,
        id: 'doc-1',
        title: 'Old lesson',
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
    storage.setItem(V1_STORAGE_KEY, v1Payload);

    expect(service.listDocuments()).toEqual([]);
    expect(storage.getItem(V1_STORAGE_KEY)).toBe(v1Payload);
  });

  it('clears malformed v2 data only', () => {
    const service = TestBed.inject(LocalDocumentStoreService);
    storage.setItem(V1_STORAGE_KEY, '[{"schemaVersion":1}]');
    storage.setItem(V2_STORAGE_KEY, 'not-json');

    expect(service.listDocuments()).toEqual([]);
    expect(storage.getItem(V1_STORAGE_KEY)).toBe('[{"schemaVersion":1}]');
    expect(storage.getItem(V2_STORAGE_KEY)).toBeNull();
  });

  it('saves v2 span documents and lists newest first', () => {
    const service = TestBed.inject(LocalDocumentStoreService);

    service.saveDocument({
      schemaVersion: 2,
      id: 'older-doc',
      title: 'Lesson 1',
      updatedAtIso: '2026-04-30T12:00:00.000Z',
      spans: [
        {
          id: 'span-1',
          kind: 'plain',
          text: 'Visit ',
        },
        {
          id: 'span-2',
          kind: 'annotated',
          sourcePinyin: 'beijing',
          text: '北京',
          displayPinyin: 'Běijīng',
          annotationScope: 'atomicPhrase',
        },
      ],
    });
    service.saveDocument({
      schemaVersion: 2,
      id: 'newer-doc',
      title: 'Lesson 2',
      updatedAtIso: '2026-05-01T12:00:00.000Z',
      spans: [
        {
          id: 'span-3',
          kind: 'plain',
          text: 'Practice',
        },
      ],
    });

    expect(service.listDocuments().map((document) => document.id)).toEqual([
      'newer-doc',
      'older-doc',
    ]);
    const storedDocuments = JSON.parse(
      storage.getItem(V2_STORAGE_KEY) ?? '[]',
    ) as unknown[];
    expect(storedDocuments).toEqual([
      {
        schemaVersion: 2,
        id: 'newer-doc',
        title: 'Lesson 2',
        updatedAtIso: '2026-05-01T12:00:00.000Z',
        spans: [
          {
            id: 'span-3',
            kind: 'plain',
            text: 'Practice',
          },
        ],
      },
      {
        schemaVersion: 2,
        id: 'older-doc',
        title: 'Lesson 1',
        updatedAtIso: '2026-04-30T12:00:00.000Z',
        spans: [
          {
            id: 'span-1',
            kind: 'plain',
            text: 'Visit ',
          },
          {
            id: 'span-2',
            kind: 'annotated',
            sourcePinyin: 'beijing',
            text: '北京',
            displayPinyin: 'Běijīng',
            annotationScope: 'atomicPhrase',
          },
        ],
      },
    ]);
    expect(JSON.stringify(storedDocuments)).not.toContain('tokens');
  });

  it('round-trips annotated character and atomic phrase scopes', () => {
    const service = TestBed.inject(LocalDocumentStoreService);

    service.saveDocument({
      schemaVersion: 2,
      id: 'doc-1',
      title: 'Scoped lesson',
      updatedAtIso: '2026-05-06T12:00:00.000Z',
      spans: [
        {
          id: 'span-character',
          kind: 'annotated',
          sourcePinyin: 'ni',
          text: '你',
          displayPinyin: 'Nǐ',
          annotationScope: 'character',
        },
        {
          id: 'span-phrase',
          kind: 'annotated',
          sourcePinyin: 'beijing',
          text: '北京',
          displayPinyin: 'Běijīng',
          annotationScope: 'atomicPhrase',
        },
      ],
    });

    expect(service.listDocuments()).toEqual([
      {
        schemaVersion: 2,
        id: 'doc-1',
        title: 'Scoped lesson',
        updatedAtIso: '2026-05-06T12:00:00.000Z',
        spans: [
          {
            id: 'span-character',
            kind: 'annotated',
            sourcePinyin: 'ni',
            text: '你',
            displayPinyin: 'Nǐ',
            annotationScope: 'character',
          },
          {
            id: 'span-phrase',
            kind: 'annotated',
            sourcePinyin: 'beijing',
            text: '北京',
            displayPinyin: 'Běijīng',
            annotationScope: 'atomicPhrase',
          },
        ],
      },
    ]);
  });

  it('loads legacy annotated spans without scopes by text width', () => {
    const service = TestBed.inject(LocalDocumentStoreService);
    storage.setItem(
      V2_STORAGE_KEY,
      JSON.stringify([
        {
          schemaVersion: 2,
          id: 'doc-1',
          title: 'Legacy lesson',
          updatedAtIso: '2026-05-06T12:00:00.000Z',
          spans: [
            {
              id: 'span-character',
              kind: 'annotated',
              sourcePinyin: 'ni',
              text: '你',
              displayPinyin: 'Nǐ',
            },
            {
              id: 'span-phrase',
              kind: 'annotated',
              sourcePinyin: 'beijing',
              text: '北京',
              displayPinyin: 'Běijīng',
            },
          ],
        },
      ]),
    );

    expect(service.listDocuments()).toEqual([
      {
        schemaVersion: 2,
        id: 'doc-1',
        title: 'Legacy lesson',
        updatedAtIso: '2026-05-06T12:00:00.000Z',
        spans: [
          {
            id: 'span-character',
            kind: 'annotated',
            sourcePinyin: 'ni',
            text: '你',
            displayPinyin: 'Nǐ',
            annotationScope: 'character',
          },
          {
            id: 'span-phrase',
            kind: 'annotated',
            sourcePinyin: 'beijing',
            text: '北京',
            displayPinyin: 'Běijīng',
            annotationScope: 'atomicPhrase',
          },
        ],
      },
    ]);
    expect(storage.getItem(V2_STORAGE_KEY)).not.toBeNull();
  });

  it('clears annotated spans with malformed scopes like other invalid spans', () => {
    const service = TestBed.inject(LocalDocumentStoreService);
    storage.setItem(
      V2_STORAGE_KEY,
      JSON.stringify([
        {
          schemaVersion: 2,
          id: 'doc-1',
          title: 'Lesson 1',
          updatedAtIso: '2026-04-30T12:00:00.000Z',
          spans: [
            {
              id: 'span-1',
              kind: 'annotated',
              sourcePinyin: 'beijing',
              text: '北京',
              displayPinyin: 'Běijīng',
              annotationScope: 'phrase',
            },
          ],
        },
      ]),
    );

    expect(service.listDocuments()).toEqual([]);
    expect(storage.getItem(V2_STORAGE_KEY)).toBeNull();
  });

  it('keeps valid documents when another stored document is invalid', () => {
    const service = TestBed.inject(LocalDocumentStoreService);
    storage.setItem(
      V2_STORAGE_KEY,
      JSON.stringify([
        {
          schemaVersion: 2,
          id: 'valid-doc',
          title: 'Lesson 1',
          updatedAtIso: '2026-04-30T12:00:00.000Z',
          spans: [{ id: 'span-1', kind: 'plain', text: 'Practice' }],
        },
        {
          schemaVersion: 2,
          id: 'invalid-doc',
          title: 'Lesson 2',
          updatedAtIso: '2026-05-01T12:00:00.000Z',
          spans: [
            {
              id: 'span-2',
              kind: 'annotated',
              sourcePinyin: 'beijing',
              text: '北京',
            },
          ],
        },
      ]),
    );

    expect(service.listDocuments()).toEqual([
      {
        schemaVersion: 2,
        id: 'valid-doc',
        title: 'Lesson 1',
        updatedAtIso: '2026-04-30T12:00:00.000Z',
        spans: [{ id: 'span-1', kind: 'plain', text: 'Practice' }],
      },
    ]);
    expect(storage.getItem(V2_STORAGE_KEY)).toBe(
      JSON.stringify([
        {
          schemaVersion: 2,
          id: 'valid-doc',
          title: 'Lesson 1',
          spans: [{ id: 'span-1', kind: 'plain', text: 'Practice' }],
          updatedAtIso: '2026-04-30T12:00:00.000Z',
        },
      ]),
    );
  });

  it('clears invalid v2 span payloads', () => {
    const service = TestBed.inject(LocalDocumentStoreService);
    storage.setItem(
      V2_STORAGE_KEY,
      JSON.stringify([
        {
          schemaVersion: 2,
          id: 'doc-1',
          title: 'Lesson 1',
          updatedAtIso: '2026-04-30T12:00:00.000Z',
          spans: [
            {
              id: 'span-1',
              kind: 'annotated',
              sourcePinyin: 'beijing',
              text: '北京',
            },
          ],
        },
      ]),
    );

    expect(service.listDocuments()).toEqual([]);
    expect(storage.getItem(V2_STORAGE_KEY)).toBeNull();
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
