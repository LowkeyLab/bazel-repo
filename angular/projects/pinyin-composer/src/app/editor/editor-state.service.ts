import { computed, Injectable, signal } from '@angular/core';

import type {
  AnnotatedSpan,
  Candidate,
  ComposerDocument,
  DocumentRange,
  DocumentSpan,
  PhraseToken,
  PlainTextSpan,
} from './phrase-token';

@Injectable({ providedIn: 'root' })
export class EditorStateService {
  private readonly inputBufferSignal = signal('');
  private readonly spansSignal = signal<readonly DocumentSpan[]>([]);
  private readonly pendingRangeSignal = signal<DocumentRange | null>(null);

  readonly inputBuffer = this.inputBufferSignal.asReadonly();
  readonly spans = this.spansSignal.asReadonly();
  readonly pendingRange = this.pendingRangeSignal.asReadonly();
  readonly documentText = computed(() =>
    this.spansSignal()
      .map((span) => span.text)
      .join(''),
  );
  readonly tokens = computed<readonly PhraseToken[]>(() =>
    this.spansSignal()
      .filter((span): span is AnnotatedSpan => span.kind === 'annotated')
      .map((span) => ({
        id: span.id,
        sourcePinyin: span.sourcePinyin,
        hanzi: span.text,
        displayPinyin: span.displayPinyin,
      })),
  );
  readonly hasContent = computed(
    () =>
      this.inputBufferSignal().trim().length > 0 ||
      this.documentText().length > 0,
  );

  updateInputBuffer(value: string): void {
    this.inputBufferSignal.set(value);
  }

  setPendingRange(range: DocumentRange | null): void {
    this.pendingRangeSignal.set(
      range ? this.normalizeRange(range.startOffset, range.endOffset) : null,
    );
  }

  replaceRange(
    startOffset: number,
    endOffset: number,
    text: string,
  ): DocumentRange {
    const normalizedRange = this.normalizeRange(startOffset, endOffset);
    let replacementRange = {
      startOffset: normalizedRange.startOffset,
      endOffset: normalizedRange.startOffset + text.length,
    };

    this.spansSignal.update((spans) => {
      const range = this.expandRangeForAtomicAnnotations(
        spans,
        normalizedRange,
      );
      replacementRange = {
        startOffset: range.startOffset,
        endOffset: range.startOffset + text.length,
      };
      const usedSpanIds = this.usedSpanIds(spans);
      const replacement =
        text.length > 0
          ? [this.toPlainSpan(text, this.createUniqueSpanId(usedSpanIds))]
          : [];
      const splitSpans = this.splitSpansForRange(spans, range, usedSpanIds);

      return this.mergeAdjacentPlainSpans([
        ...splitSpans.before,
        ...replacement,
        ...splitSpans.after,
      ]);
    });

    return replacementRange;
  }

  commitCandidateToRange(range: DocumentRange, candidate: Candidate): void {
    const normalizedRange = this.normalizeRange(
      range.startOffset,
      range.endOffset,
    );
    this.spansSignal.update((spans) => {
      const expandedRange = this.expandRangeForAtomicAnnotations(
        spans,
        normalizedRange,
      );
      const usedSpanIds = this.usedSpanIds(spans);
      const replacement = this.spansForCandidate(candidate, usedSpanIds);
      const splitSpans = this.splitSpansForRange(
        spans,
        expandedRange,
        usedSpanIds,
      );

      return this.mergeAdjacentPlainSpans([
        ...splitSpans.before,
        ...replacement,
        ...splitSpans.after,
      ]);
    });
    this.inputBufferSignal.set('');
    this.pendingRangeSignal.set(null);
  }

  loadDocument(document: ComposerDocument): void {
    this.spansSignal.set([...document.spans]);
    this.inputBufferSignal.set('');
    this.pendingRangeSignal.set(null);
  }

  loadTokens(tokens: readonly PhraseToken[]): void {
    this.spansSignal.set(
      tokens.map((token) => ({
        id: token.id,
        kind: 'annotated',
        sourcePinyin: token.sourcePinyin,
        text: token.hanzi,
        displayPinyin: token.displayPinyin,
        annotationScope: 'atomicPhrase',
      })),
    );
    this.inputBufferSignal.set('');
    this.pendingRangeSignal.set(null);
  }

  commitCandidate(candidate: Candidate): void {
    const endOffset = this.documentText().length;
    this.commitCandidateToRange(
      { startOffset: endOffset, endOffset },
      candidate,
    );
  }

  replaceToken(tokenId: string, candidate: Candidate): void {
    const range = this.findSpanRange(tokenId);
    if (!range) {
      return;
    }

    this.commitCandidateToRange(range, candidate);
  }

  clear(): void {
    this.inputBufferSignal.set('');
    this.spansSignal.set([]);
    this.pendingRangeSignal.set(null);
  }

  private normalizeRange(
    startOffset: number,
    endOffset: number,
  ): DocumentRange {
    const documentLength = this.documentText().length;
    const safeStartOffset = this.clampOffset(startOffset, documentLength);
    const safeEndOffset = this.clampOffset(endOffset, documentLength);

    return {
      startOffset: Math.min(safeStartOffset, safeEndOffset),
      endOffset: Math.max(safeStartOffset, safeEndOffset),
    };
  }

  private clampOffset(offset: number, documentLength: number): number {
    if (!Number.isFinite(offset)) {
      return 0;
    }

    return Math.min(Math.max(Math.trunc(offset), 0), documentLength);
  }

  private expandRangeForAtomicAnnotations(
    spans: readonly DocumentSpan[],
    range: DocumentRange,
  ): DocumentRange {
    let expandedRange = range;
    let didExpand = true;

    while (didExpand) {
      didExpand = false;
      let spanStartOffset = 0;

      for (const span of spans) {
        const spanEndOffset = spanStartOffset + span.text.length;
        if (
          this.rangeIntersectsSpan(
            expandedRange,
            spanStartOffset,
            spanEndOffset,
          )
        ) {
          if (
            span.kind === 'annotated' &&
            span.annotationScope === 'atomicPhrase' &&
            (spanStartOffset < expandedRange.startOffset ||
              spanEndOffset > expandedRange.endOffset)
          ) {
            expandedRange = {
              startOffset: Math.min(expandedRange.startOffset, spanStartOffset),
              endOffset: Math.max(expandedRange.endOffset, spanEndOffset),
            };
            didExpand = true;
          }
        }
        spanStartOffset = spanEndOffset;
      }
    }

    return expandedRange;
  }

  private rangeIntersectsSpan(
    range: DocumentRange,
    spanStartOffset: number,
    spanEndOffset: number,
  ): boolean {
    return (
      range.startOffset < spanEndOffset && range.endOffset > spanStartOffset
    );
  }

  private spansForCandidate(
    candidate: Candidate,
    usedSpanIds: Set<string>,
  ): readonly AnnotatedSpan[] {
    const alignedSpans = this.alignedSpansForCandidate(candidate, usedSpanIds);
    if (alignedSpans) {
      return alignedSpans;
    }

    return [
      {
        id: this.createUniqueSpanId(usedSpanIds),
        kind: 'annotated',
        sourcePinyin: candidate.sourcePinyin,
        text: candidate.hanzi,
        displayPinyin: candidate.displayPinyin,
        annotationScope: 'atomicPhrase',
      },
    ];
  }

  private alignedSpansForCandidate(
    candidate: Candidate,
    usedSpanIds: Set<string>,
  ): readonly AnnotatedSpan[] | null {
    const hanziCharacters = Array.from(candidate.hanzi);
    if (
      hanziCharacters.length === 0 ||
      !hanziCharacters.every((character) => this.isHanzi(character)) ||
      candidate.sourcePinyinSyllables.length !== hanziCharacters.length ||
      candidate.displayPinyinSyllables.length !== hanziCharacters.length ||
      !candidate.sourcePinyinSyllables.every(
        (syllable) => syllable.length > 0,
      ) ||
      !candidate.displayPinyinSyllables.every((syllable) => syllable.length > 0)
    ) {
      return null;
    }

    return hanziCharacters.map((character, index) => ({
      id: this.createUniqueSpanId(usedSpanIds),
      kind: 'annotated',
      sourcePinyin: candidate.sourcePinyinSyllables[index],
      text: character,
      displayPinyin: candidate.displayPinyinSyllables[index],
      annotationScope: 'character',
    }));
  }

  private isHanzi(character: string): boolean {
    return /^\p{Script=Han}$/u.test(character);
  }

  private splitSpansForRange(
    spans: readonly DocumentSpan[],
    range: DocumentRange,
    usedSpanIds: Set<string>,
  ): {
    readonly before: readonly DocumentSpan[];
    readonly after: readonly DocumentSpan[];
  } {
    const before: DocumentSpan[] = [];
    const after: DocumentSpan[] = [];
    let spanStartOffset = 0;

    for (const span of spans) {
      const spanEndOffset = spanStartOffset + span.text.length;
      if (spanEndOffset <= range.startOffset) {
        before.push(span);
        spanStartOffset = spanEndOffset;
        continue;
      }

      if (spanStartOffset >= range.endOffset) {
        after.push(span);
        spanStartOffset = spanEndOffset;
        continue;
      }

      const leadingEndOffset = this.clampSpanOffset(
        range.startOffset - spanStartOffset,
        span.text.length,
      );
      const trailingStartOffset = this.clampSpanOffset(
        range.endOffset - spanStartOffset,
        span.text.length,
      );
      const hasLeadingFragment = leadingEndOffset > 0;
      const hasTrailingFragment = trailingStartOffset < span.text.length;

      if (hasLeadingFragment) {
        before.push(this.sliceSpan(span, 0, leadingEndOffset, span.id));
      }
      if (hasTrailingFragment) {
        after.push(
          this.sliceSpan(
            span,
            trailingStartOffset,
            span.text.length,
            hasLeadingFragment ? this.createUniqueSpanId(usedSpanIds) : span.id,
          ),
        );
      }
      spanStartOffset = spanEndOffset;
    }

    return {
      before: before.filter((span) => span.text.length > 0),
      after: after.filter((span) => span.text.length > 0),
    };
  }

  private sliceSpan(
    span: DocumentSpan,
    startOffset: number,
    endOffset = span.text.length,
    id = span.id,
  ): DocumentSpan {
    const text = span.text.slice(startOffset, endOffset);
    if (span.kind === 'plain') {
      return this.toPlainSpan(text, id);
    }

    return {
      ...span,
      id,
      text,
    };
  }

  private clampSpanOffset(offset: number, spanLength: number): number {
    return Math.min(Math.max(offset, 0), spanLength);
  }

  private mergeAdjacentPlainSpans(
    spans: readonly DocumentSpan[],
  ): readonly DocumentSpan[] {
    const mergedSpans: DocumentSpan[] = [];

    for (const span of spans) {
      if (span.text.length === 0) {
        continue;
      }

      const previousSpan = mergedSpans.at(-1);
      if (previousSpan?.kind === 'plain' && span.kind === 'plain') {
        mergedSpans[mergedSpans.length - 1] = this.toPlainSpan(
          `${previousSpan.text}${span.text}`,
          previousSpan.id,
        );
      } else {
        mergedSpans.push(span);
      }
    }

    return mergedSpans;
  }

  private toPlainSpan(text: string, id: string): PlainTextSpan {
    return {
      id,
      kind: 'plain',
      text,
    };
  }

  private usedSpanIds(spans: readonly DocumentSpan[]): Set<string> {
    return new Set(spans.map((span) => span.id));
  }

  private createUniqueSpanId(usedSpanIds: Set<string>): string {
    let id = crypto.randomUUID();
    while (usedSpanIds.has(id)) {
      id = crypto.randomUUID();
    }
    usedSpanIds.add(id);

    return id;
  }

  private findSpanRange(spanId: string): DocumentRange | null {
    let spanStartOffset = 0;

    for (const span of this.spansSignal()) {
      const spanEndOffset = spanStartOffset + span.text.length;
      if (span.id === spanId) {
        return { startOffset: spanStartOffset, endOffset: spanEndOffset };
      }
      spanStartOffset = spanEndOffset;
    }

    return null;
  }
}
