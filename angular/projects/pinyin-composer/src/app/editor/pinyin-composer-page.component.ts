import {
  ChangeDetectionStrategy,
  Component,
  inject,
  signal,
} from '@angular/core';
import { FormsModule } from '@angular/forms';

import { LocalDocumentStoreService } from '../documents/local-document-store.service';
import { ConversionWorkerClient } from '../wasm/conversion-worker.client';
import {
  DocumentEditorComponent,
  type DocumentTextReplacement,
} from './document-editor.component';
import { EditorStateService } from './editor-state.service';
import { InlineCandidateMenuComponent } from './inline-candidate-menu.component';
import type { Candidate } from './phrase-token';

@Component({
  selector: 'app-pinyin-composer-page',
  imports: [FormsModule, DocumentEditorComponent, InlineCandidateMenuComponent],
  template: `
    <main class="composer-page">
      <header>
        <h1>Pinyin Composer</h1>
        <p>
          Type tone-free pinyin directly in the document, commit Hanzi phrase
          spans, and save a local annotated draft.
        </p>
      </header>

      <label for="document-title">Document title</label>
      <input
        id="document-title"
        data-testid="document-title"
        [ngModel]="documentTitle()"
        (ngModelChange)="documentTitle.set($event)"
      />

      <label id="document-editor-label">Document body</label>
      <app-document-editor
        class="document-editor-host"
        aria-labelledby="document-editor-label"
        [spans]="editor.spans()"
        (textReplaced)="replaceDocumentText($event)"
        (keydown.enter)="commitTopCandidate($event)"
      />

      @if (conversionError()) {
        <p class="error" data-testid="conversion-error">
          {{ conversionError() }}
        </p>
      }

      <app-inline-candidate-menu
        [candidates]="candidates()"
        (candidateSelected)="commitCandidate($event)"
      />

      <button
        type="button"
        data-testid="save-document"
        (click)="saveCurrentDocument()"
      >
        Save Draft
      </button>
    </main>
  `,
  styles: [
    `
      .composer-page {
        --composer-page-ink: #0f172a;
        --composer-page-muted: #475569;
        --composer-page-border: #cbd5e1;
        --composer-page-error: #b91c1c;
        --composer-page-surface: #ffffff;
        --composer-page-space-sm: 0.5rem;
        --composer-page-space-md: 0.75rem;
        --composer-page-space-button-x: 0.9rem;
        --composer-page-space-lg: 1rem;
        --composer-page-space-xl: 2rem;
        --composer-page-radius-md: 0.75rem;
        --composer-page-radius-pill: 999px;
        --composer-page-measure: 56rem;
        max-width: var(--composer-page-measure);
        margin: 0 auto;
        padding: var(--composer-page-space-xl);
        font-family: system-ui, sans-serif;
      }

      header p {
        color: var(--composer-page-muted);
      }

      input {
        box-sizing: border-box;
        width: 100%;
        margin: var(--composer-page-space-sm) 0 var(--composer-page-space-lg);
        padding: var(--composer-page-space-md);
        border: 1px solid var(--composer-page-border);
        border-radius: var(--composer-page-radius-md);
        font: inherit;
      }

      .document-editor-host {
        display: block;
        margin: var(--composer-page-space-sm) 0 var(--composer-page-space-lg);
      }

      button {
        border: 1px solid var(--composer-page-ink);
        border-radius: var(--composer-page-radius-pill);
        background: var(--composer-page-ink);
        color: var(--composer-page-surface);
        padding: var(--composer-page-space-sm)
          var(--composer-page-space-button-x);
        cursor: pointer;
      }

      .error {
        color: var(--composer-page-error);
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class PinyinComposerPageComponent {
  readonly editor = inject(EditorStateService);
  private readonly conversion = inject(ConversionWorkerClient);
  private readonly documents = inject(LocalDocumentStoreService);

  readonly candidates = signal<readonly Candidate[]>([]);
  readonly conversionError = signal('');
  readonly documentId = signal(crypto.randomUUID());
  readonly documentTitle = signal('Untitled pinyin document');
  private conversionRequestId = 0;

  async replaceDocumentText(
    replacement: DocumentTextReplacement,
  ): Promise<void> {
    const previousPendingRange = this.editor.pendingRange();
    const previousInputBuffer = this.editor.inputBuffer();
    const continuesPendingRun =
      previousPendingRange !== null &&
      replacement.startOffset === replacement.endOffset &&
      replacement.text.length > 0 &&
      replacement.text !== '\n' &&
      replacement.startOffset === previousPendingRange.endOffset;

    const replacementRange = this.editor.replaceRange(
      replacement.startOffset,
      replacement.endOffset,
      replacement.text,
    );

    const inputBuffer = continuesPendingRun
      ? `${previousInputBuffer}${replacement.text}`
      : replacement.text;
    const trimmedInputBuffer = inputBuffer.trim();
    const pendingRange =
      trimmedInputBuffer.length === 0 || replacement.text === '\n'
        ? null
        : {
            startOffset: continuesPendingRun
              ? previousPendingRange.startOffset
              : replacementRange.startOffset,
            endOffset: replacementRange.endOffset,
          };

    this.editor.setPendingRange(pendingRange);
    this.editor.updateInputBuffer(pendingRange ? inputBuffer : '');
    this.conversionError.set('');
    const requestId = ++this.conversionRequestId;

    if (!trimmedInputBuffer || !replacement.text.trim()) {
      this.candidates.set([]);
      return;
    }

    try {
      const candidates = await this.conversion.convertPinyin(
        trimmedInputBuffer,
        5,
      );
      if (requestId !== this.conversionRequestId) {
        return;
      }
      this.candidates.set(candidates);
    } catch (error: unknown) {
      if (requestId !== this.conversionRequestId) {
        return;
      }
      this.candidates.set([]);
      this.conversionError.set(
        error instanceof Error ? error.message : String(error),
      );
    }
  }

  commitTopCandidate(event: Event): void {
    const [topCandidate] = this.candidates();
    if (topCandidate) {
      event.preventDefault();
      this.commitCandidate(topCandidate);
    }
  }

  commitCandidate(candidate: Candidate): void {
    const pendingRange = this.editor.pendingRange();
    if (!pendingRange) {
      this.candidates.set([]);
      return;
    }
    this.editor.commitCandidateToRange(pendingRange, candidate);
    this.candidates.set([]);
  }

  saveCurrentDocument(): void {
    this.documents.saveDocument({
      schemaVersion: 2,
      id: this.documentId(),
      title: this.documentTitle().trim() || 'Untitled pinyin document',
      spans: this.editor.spans(),
      updatedAtIso: new Date().toISOString(),
    });
  }
}
