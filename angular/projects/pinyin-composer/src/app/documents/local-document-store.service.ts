import { Injectable, InjectionToken, inject } from '@angular/core';

import type { ComposerDocument } from '../editor/phrase-token';

export const LOCAL_DOCUMENT_STORAGE = new InjectionToken<Storage>(
  'LOCAL_DOCUMENT_STORAGE',
  {
    providedIn: 'root',
    factory: () => window.localStorage,
  },
);

@Injectable({ providedIn: 'root' })
export class LocalDocumentStoreService {
  private readonly storage = inject(LOCAL_DOCUMENT_STORAGE);
  private readonly storageKey = 'pinyin-composer.documents.v1';

  listDocuments(): readonly ComposerDocument[] {
    const raw = this.storage.getItem(this.storageKey);
    if (!raw) {
      return [];
    }

    let parsed: unknown;
    try {
      parsed = JSON.parse(raw);
    } catch {
      this.storage.removeItem(this.storageKey);
      return [];
    }

    if (!Array.isArray(parsed)) {
      this.storage.removeItem(this.storageKey);
      return [];
    }

    return [...parsed].sort((left, right) =>
      right.updatedAtIso.localeCompare(left.updatedAtIso),
    );
  }

  saveDocument(document: ComposerDocument): void {
    const documents = this.listDocuments().filter(
      (item) => item.id !== document.id,
    );
    this.storage.setItem(
      this.storageKey,
      JSON.stringify([document, ...documents]),
    );
  }

  deleteDocument(documentId: string): void {
    this.storage.setItem(
      this.storageKey,
      JSON.stringify(
        this.listDocuments().filter((document) => document.id !== documentId),
      ),
    );
  }
}
