import { Injectable, InjectionToken, inject } from '@angular/core';

import { ComposerDocument } from '../editor/phrase-token';

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

    const parsed = JSON.parse(raw) as readonly ComposerDocument[];
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
