import { ChangeDetectionStrategy, Component, input } from '@angular/core';

import type { DocumentSpan } from './phrase-token';

@Component({
  selector: 'app-pdf-export-document',
  template: `
    <div class="pdf-export-document" data-testid="pdf-export-document">
      @for (span of spans(); track span.id) {
        @if (span.kind === 'annotated') {
          @for (fragment of splitText(span.text); track fragment.index) {
            @if (fragment.kind === 'text') {
              <ruby data-testid="pdf-export-ruby">
                <rb>{{ fragment.text }}</rb>
                <rt>{{ span.displayPinyin }}</rt>
              </ruby>
            } @else {
              <br data-testid="pdf-export-line-break" />
            }
          }
        } @else {
          @for (fragment of splitText(span.text); track fragment.index) {
            @if (fragment.kind === 'text') {
              <span data-testid="pdf-export-plain">{{ fragment.text }}</span>
            } @else {
              <br data-testid="pdf-export-line-break" />
            }
          }
        }
      }
    </div>
  `,
  styles: [
    `
      :host {
        display: block;
      }

      .pdf-export-document {
        white-space: pre-wrap;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class PdfExportDocumentComponent {
  readonly spans = input.required<readonly DocumentSpan[]>();

  protected splitText(text: string): readonly PdfExportFragment[] {
    const fragments: PdfExportFragment[] = [];
    let startIndex = 0;

    for (let index = 0; index < text.length; index += 1) {
      if (text[index] === '\n') {
        if (index > startIndex) {
          fragments.push({
            kind: 'text',
            index: fragments.length,
            text: text.slice(startIndex, index),
          });
        }
        fragments.push({ kind: 'line-break', index: fragments.length });
        startIndex = index + 1;
      }
    }

    if (startIndex < text.length) {
      fragments.push({
        kind: 'text',
        index: fragments.length,
        text: text.slice(startIndex),
      });
    }

    return fragments;
  }
}

type PdfExportFragment = PdfExportTextFragment | PdfExportLineBreakFragment;

interface PdfExportTextFragment {
  readonly kind: 'text';
  readonly index: number;
  readonly text: string;
}

interface PdfExportLineBreakFragment {
  readonly kind: 'line-break';
  readonly index: number;
}
