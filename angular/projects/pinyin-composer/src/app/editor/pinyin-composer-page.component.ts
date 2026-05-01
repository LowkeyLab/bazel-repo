import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  signal,
} from '@angular/core';
import { FormsModule } from '@angular/forms';

import { LocalDocumentStoreService } from '../documents/local-document-store.service';
import { HtmlRubyExportService } from '../export/html-ruby-export.service';
import { ConversionWorkerClient } from '../wasm/conversion-worker.client';
import { EditorStateService } from './editor-state.service';
import { InlineCandidateMenuComponent } from './inline-candidate-menu.component';
import type { Candidate } from './phrase-token';
import { RubyPreviewComponent } from './ruby-preview.component';

@Component({
  selector: 'app-pinyin-composer-page',
  imports: [FormsModule, InlineCandidateMenuComponent, RubyPreviewComponent],
  template: `
    <main class="composer-page">
      <header>
        <h1>Pinyin Composer</h1>
        <p>
          Type tone-free pinyin, commit Hanzi phrase tokens, and export
          phrase-level ruby HTML.
        </p>
      </header>

      <label for="document-title">Document title</label>
      <input
        id="document-title"
        data-testid="document-title"
        [ngModel]="documentTitle()"
        (ngModelChange)="documentTitle.set($event)"
      />

      <label for="pinyin-input">Pinyin input</label>
      <textarea
        id="pinyin-input"
        data-testid="pinyin-input"
        [ngModel]="editor.inputBuffer()"
        (ngModelChange)="onInputChange($event)"
        (keydown.enter)="commitTopCandidate($event)"
        placeholder="wo xiang qu beijing"
      ></textarea>

      @if (conversionError()) {
        <p class="error" data-testid="conversion-error">
          {{ conversionError() }}
        </p>
      }

      <app-inline-candidate-menu
        [candidates]="candidates()"
        (candidateSelected)="commitCandidate($event)"
      />
      <app-ruby-preview
        [tokens]="editor.tokens()"
        (tokenSelected)="selectTokenForCorrection($event)"
      />

      <button
        type="button"
        data-testid="save-document"
        (click)="saveCurrentDocument()"
      >
        Save Draft
      </button>

      <section>
        <h2>HTML ruby export</h2>
        <textarea
          readonly
          data-testid="html-export"
          [value]="htmlExport()"
        ></textarea>
      </section>
    </main>
  `,
  styles: [
    `
      .composer-page {
        max-width: 56rem;
        margin: 0 auto;
        padding: 2rem;
        font-family: system-ui, sans-serif;
      }

      input,
      textarea {
        box-sizing: border-box;
        width: 100%;
        margin: 0.5rem 0 1rem;
        padding: 0.75rem;
        border: 1px solid #cbd5e1;
        border-radius: 0.75rem;
        font: inherit;
      }

      textarea {
        min-height: 8rem;
      }

      button {
        border: 1px solid #0f172a;
        border-radius: 999px;
        background: #0f172a;
        color: white;
        padding: 0.5rem 0.9rem;
        cursor: pointer;
      }

      .error {
        color: #b91c1c;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class PinyinComposerPageComponent {
  readonly editor = inject(EditorStateService);
  private readonly conversion = inject(ConversionWorkerClient);
  private readonly exporter = inject(HtmlRubyExportService);
  private readonly documents = inject(LocalDocumentStoreService);

  readonly candidates = signal<readonly Candidate[]>([]);
  readonly conversionError = signal('');
  readonly correctionTokenId = signal<string | null>(null);
  readonly documentId = signal(crypto.randomUUID());
  readonly documentTitle = signal('Untitled pinyin document');
  readonly htmlExport = computed(() =>
    this.exporter.exportTokens(this.editor.tokens()),
  );
  private conversionRequestId = 0;

  async onInputChange(value: string): Promise<void> {
    this.editor.updateInputBuffer(value);
    this.conversionError.set('');
    const requestId = ++this.conversionRequestId;

    const trimmed = value.trim();
    if (!trimmed) {
      this.candidates.set([]);
      return;
    }

    try {
      const candidates = await this.conversion.convertPinyin(trimmed, 5);
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
    event.preventDefault();
    const [topCandidate] = this.candidates();
    if (topCandidate) {
      this.commitCandidate(topCandidate);
    }
  }

  commitCandidate(candidate: Candidate): void {
    const tokenId = this.correctionTokenId();
    if (tokenId) {
      this.editor.replaceToken(tokenId, candidate);
      this.correctionTokenId.set(null);
    } else {
      this.editor.commitCandidate(candidate);
    }
    this.candidates.set([]);
  }

  async selectTokenForCorrection(tokenId: string): Promise<void> {
    const token = this.editor.tokens().find((item) => item.id === tokenId);
    if (!token) {
      return;
    }

    this.correctionTokenId.set(tokenId);
    this.editor.updateInputBuffer(token.sourcePinyin);
    this.conversionError.set('');
    const requestId = ++this.conversionRequestId;
    try {
      const candidates = await this.conversion.convertPinyin(
        token.sourcePinyin,
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

  saveCurrentDocument(): void {
    this.documents.saveDocument({
      id: this.documentId(),
      title: this.documentTitle().trim() || 'Untitled pinyin document',
      tokens: this.editor.tokens(),
      updatedAtIso: new Date().toISOString(),
    });
  }
}
