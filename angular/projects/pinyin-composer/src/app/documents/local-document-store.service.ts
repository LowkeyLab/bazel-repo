import { Injectable, InjectionToken, inject } from '@angular/core';

import type { ComposerDocument, DocumentSpan } from '../editor/phrase-token';

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
  private readonly storageKey = 'pinyin-composer.documents.v2';

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

    const documents = parsed.map((item) => parseComposerDocument(item));
    if (documents.includes(null)) {
      this.storage.removeItem(this.storageKey);
      return [];
    }

    return documents
      .filter((document): document is ComposerDocument => document !== null)
      .sort((left, right) =>
        right.updatedAtIso.localeCompare(left.updatedAtIso),
      );
  }

  saveDocument(document: ComposerDocument): void {
    const documents = this.listDocuments().filter(
      (item) => item.id !== document.id,
    );
    this.storage.setItem(
      this.storageKey,
      JSON.stringify([normalizeComposerDocument(document), ...documents]),
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

type ComposerDocumentPayload = {
  readonly schemaVersion?: unknown;
  readonly id?: unknown;
  readonly title?: unknown;
  readonly spans?: unknown;
  readonly updatedAtIso?: unknown;
};

type DocumentSpanPayload = {
  readonly id?: unknown;
  readonly kind?: unknown;
  readonly sourcePinyin?: unknown;
  readonly text?: unknown;
  readonly displayPinyin?: unknown;
  readonly annotationScope?: unknown;
};

function parseComposerDocument(value: unknown): ComposerDocument | null {
  if (!isObject(value)) {
    return null;
  }

  const document = value as ComposerDocumentPayload;
  if (
    document.schemaVersion !== 2 ||
    typeof document.id !== 'string' ||
    typeof document.title !== 'string' ||
    typeof document.updatedAtIso !== 'string' ||
    !Array.isArray(document.spans)
  ) {
    return null;
  }

  const spans = document.spans.map((span) => parseDocumentSpan(span));
  if (spans.includes(null)) {
    return null;
  }

  return {
    schemaVersion: 2,
    id: document.id,
    title: document.title,
    spans: spans.filter((span): span is DocumentSpan => span !== null),
    updatedAtIso: document.updatedAtIso,
  };
}

function parseDocumentSpan(value: unknown): DocumentSpan | null {
  if (!isObject(value)) {
    return null;
  }

  const span = value as DocumentSpanPayload;
  if (
    span.kind === 'plain' &&
    typeof span.id === 'string' &&
    typeof span.text === 'string'
  ) {
    return {
      id: span.id,
      kind: 'plain',
      text: span.text,
    };
  }

  if (
    span.kind === 'annotated' &&
    typeof span.id === 'string' &&
    typeof span.sourcePinyin === 'string' &&
    typeof span.text === 'string' &&
    typeof span.displayPinyin === 'string'
  ) {
    const annotationScope = parseAnnotationScope(span);
    if (!annotationScope) {
      return null;
    }

    return {
      id: span.id,
      kind: 'annotated',
      sourcePinyin: span.sourcePinyin,
      text: span.text,
      displayPinyin: span.displayPinyin,
      annotationScope,
    };
  }

  return null;
}

function parseAnnotationScope(
  span: DocumentSpanPayload,
): 'character' | 'atomicPhrase' | null {
  if (
    span.annotationScope === 'character' ||
    span.annotationScope === 'atomicPhrase'
  ) {
    return span.annotationScope;
  }

  if (span.annotationScope === undefined && typeof span.text === 'string') {
    return span.text.length === 1 ? 'character' : 'atomicPhrase';
  }

  return null;
}

function normalizeComposerDocument(
  document: ComposerDocument,
): ComposerDocument {
  return {
    schemaVersion: 2,
    id: document.id,
    title: document.title,
    spans: document.spans.map((span) => ({ ...span })),
    updatedAtIso: document.updatedAtIso,
  };
}

function isObject(value: unknown): value is object {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
