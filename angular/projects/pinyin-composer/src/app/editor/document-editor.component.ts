import {
  type AfterViewChecked,
  ChangeDetectionStrategy,
  Component,
  type ElementRef,
  input,
  output,
  viewChild,
} from '@angular/core';

import type { DocumentSpan } from './phrase-token';

export interface DocumentTextReplacement {
  readonly startOffset: number;
  readonly endOffset: number;
  readonly text: string;
}

interface DocumentSelectionRange {
  readonly startOffset: number;
  readonly endOffset: number;
}

@Component({
  selector: 'app-document-editor',
  template: `
    <div
      #editorRoot
      class="document-editor"
      contenteditable="true"
      role="textbox"
      aria-multiline="true"
      data-testid="document-editor"
      (beforeinput)="onBeforeInput($event)"
      (compositionstart)="onCompositionStart()"
      (compositionend)="onCompositionEnd($event)"
    >
      @for (span of spans(); track span.id) {
        @if (span.kind === 'annotated') {
          <span
            class="document-span annotated-span"
            data-testid="annotated-span"
            [attr.data-span-id]="span.id"
          >
            <ruby
              ><rb data-editor-text>{{ span.text }}</rb
              ><rt contenteditable="false">{{ span.displayPinyin }}</rt></ruby
            >
          </span>
        } @else {
          <span
            class="document-span plain-span"
            data-testid="plain-span"
            data-editor-text
            [attr.data-span-id]="span.id"
            >{{ span.text }}</span
          >
        }
      }
    </div>
  `,
  styles: [
    `
      :host {
        --document-editor-surface: #ffffff;
        --document-editor-ink: #0f172a;
        --document-editor-muted: #475569;
        --document-editor-border: #cbd5e1;
        --document-editor-focus: #2563eb;
        --document-editor-gap: 0.25rem;
        --document-editor-padding: 1rem;
        --document-editor-radius: 0.75rem;
        --document-editor-font-size: 1.5rem;
        --document-editor-line-height: 2.2;
        --document-editor-ruby-scale: 0.45em;
        --document-editor-transition: 160ms ease;
        display: block;
      }

      .document-editor {
        box-sizing: border-box;
        min-height: 10rem;
        width: 100%;
        border: 1px solid var(--document-editor-border);
        border-radius: var(--document-editor-radius);
        background: var(--document-editor-surface);
        color: var(--document-editor-ink);
        padding: var(--document-editor-padding);
        font: inherit;
        font-size: var(--document-editor-font-size);
        line-height: var(--document-editor-line-height);
        white-space: pre-wrap;
        outline: none;
        transition:
          border-color var(--document-editor-transition),
          box-shadow var(--document-editor-transition);
      }

      .document-editor:focus {
        border-color: var(--document-editor-focus);
        box-shadow: 0 0 0 var(--document-editor-gap)
          color-mix(in srgb, var(--document-editor-focus) 18%, transparent);
      }

      .document-span {
        display: inline;
      }

      ruby {
        ruby-align: center;
      }

      rt {
        color: var(--document-editor-muted);
        font-size: var(--document-editor-ruby-scale);
        user-select: none;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class DocumentEditorComponent implements AfterViewChecked {
  readonly spans = input.required<readonly DocumentSpan[]>();
  readonly textReplaced = output<DocumentTextReplacement>();

  private readonly editorRoot =
    viewChild.required<ElementRef<HTMLElement>>('editorRoot');
  private compositionStartRange: DocumentSelectionRange | null = null;
  private isComposing = false;
  private pendingCaretOffset: number | null = null;
  private pendingSelectionRange: DocumentSelectionRange | null = null;

  ngAfterViewChecked(): void {
    if (this.pendingCaretOffset === null) {
      return;
    }

    const caretOffset = this.pendingCaretOffset;
    this.pendingCaretOffset = null;
    this.pendingSelectionRange = null;
    this.restoreCollapsedSelection(caretOffset);
  }

  onBeforeInput(event: InputEvent): void {
    if (this.isComposing) {
      if (event.cancelable) {
        event.preventDefault();
      }
      return;
    }

    const replacement = this.replacementForInput(event);
    if (!replacement) {
      return;
    }

    event.preventDefault();
    this.storePendingCollapsedSelection(
      replacement.startOffset + replacement.text.length,
    );
    this.textReplaced.emit(replacement);
  }

  onCompositionStart(): void {
    this.isComposing = true;
    this.compositionStartRange = this.selectionRangeForInput();
  }

  onCompositionEnd(event: CompositionEvent): void {
    this.isComposing = false;
    const range = this.compositionStartRange ?? this.currentSelectionRange();
    this.compositionStartRange = null;
    if (!event.data || !range) {
      return;
    }

    this.storePendingCollapsedSelection(range.startOffset + event.data.length);
    this.textReplaced.emit({
      ...range,
      text: event.data,
    });
  }

  private selectionRangeForInput(): DocumentSelectionRange | null {
    const currentRange = this.currentSelectionRange();
    if (
      this.pendingSelectionRange &&
      currentRange &&
      currentRange.startOffset === currentRange.endOffset
    ) {
      return this.pendingSelectionRange;
    }

    return currentRange ?? this.pendingSelectionRange;
  }

  private storePendingCollapsedSelection(offset: number): void {
    this.pendingCaretOffset = offset;
    this.pendingSelectionRange = { startOffset: offset, endOffset: offset };
  }

  private replacementForInput(
    event: InputEvent,
  ): DocumentTextReplacement | null {
    const range = this.selectionRangeForInput();
    if (!range) {
      return null;
    }

    if (event.inputType === 'deleteContentBackward') {
      return this.deleteBackwardReplacement(range);
    }

    if (event.inputType === 'deleteContentForward') {
      return this.deleteForwardReplacement(range);
    }

    if (event.inputType.startsWith('delete')) {
      return { ...range, text: '' };
    }

    if (
      event.inputType === 'insertParagraph' ||
      event.inputType === 'insertLineBreak'
    ) {
      return { ...range, text: '\n' };
    }

    return event.data && event.data.length > 0
      ? { ...range, text: event.data }
      : null;
  }

  private deleteBackwardReplacement(
    range: DocumentSelectionRange,
  ): DocumentTextReplacement {
    if (range.startOffset !== range.endOffset) {
      return { ...range, text: '' };
    }

    return {
      startOffset: Math.max(0, range.startOffset - 1),
      endOffset: range.endOffset,
      text: '',
    };
  }

  private deleteForwardReplacement(
    range: DocumentSelectionRange,
  ): DocumentTextReplacement {
    if (range.startOffset !== range.endOffset) {
      return { ...range, text: '' };
    }

    return {
      startOffset: range.startOffset,
      endOffset: Math.min(this.documentTextLength(), range.endOffset + 1),
      text: '',
    };
  }

  private documentTextLength(): number {
    return this.spans().reduce((length, span) => length + span.text.length, 0);
  }

  private currentSelectionRange(): DocumentSelectionRange | null {
    const selection = window.getSelection();
    if (!selection?.anchorNode || !selection.focusNode) {
      return null;
    }

    const startOffset = this.offsetForBoundary(
      selection.anchorNode,
      selection.anchorOffset,
    );
    const endOffset = this.offsetForBoundary(
      selection.focusNode,
      selection.focusOffset,
    );
    if (startOffset === null || endOffset === null) {
      return null;
    }

    return {
      startOffset: Math.min(startOffset, endOffset),
      endOffset: Math.max(startOffset, endOffset),
    };
  }

  private offsetForBoundary(container: Node, offset: number): number | null {
    const root = this.editorRoot().nativeElement;
    if (!this.isInsideEditor(root, container)) {
      return null;
    }

    const range = document.createRange();
    range.setStart(root, 0);
    range.setEnd(container, offset);

    return this.baseTextLength(range.cloneContents());
  }

  private restoreCollapsedSelection(offset: number): void {
    const boundary = this.boundaryForOffset(offset);
    const selection = window.getSelection();
    if (!selection || !boundary) {
      return;
    }

    const range = document.createRange();
    range.setStart(boundary.container, boundary.offset);
    range.collapse(true);
    selection.removeAllRanges();
    selection.addRange(range);
  }

  private boundaryForOffset(offset: number): {
    readonly container: Node;
    readonly offset: number;
  } | null {
    const root = this.editorRoot().nativeElement;
    const safeOffset = Math.max(0, Math.trunc(offset));
    let remainingOffset = safeOffset;
    let lastTextNode: Text | null = null;

    for (const textContainer of Array.from(
      root.querySelectorAll('[data-editor-text]'),
    )) {
      for (const textNode of this.textNodes(textContainer)) {
        lastTextNode = textNode;
        if (remainingOffset <= textNode.length) {
          return { container: textNode, offset: remainingOffset };
        }
        remainingOffset -= textNode.length;
      }
    }

    if (lastTextNode) {
      return { container: lastTextNode, offset: lastTextNode.length };
    }

    return { container: root, offset: 0 };
  }

  private textNodes(root: Element): Text[] {
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    const nodes: Text[] = [];
    let node = walker.nextNode();
    while (node) {
      nodes.push(node as Text);
      node = walker.nextNode();
    }

    return nodes;
  }

  private isInsideEditor(root: HTMLElement, node: Node): boolean {
    return node === root || root.contains(node);
  }

  private baseTextLength(fragment: DocumentFragment): number {
    return Array.from(fragment.querySelectorAll('[data-editor-text]')).reduce(
      (length, element) => length + (element.textContent?.length ?? 0),
      0,
    );
  }
}
