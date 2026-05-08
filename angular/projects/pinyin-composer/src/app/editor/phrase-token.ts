export interface Candidate {
  readonly id: string;
  readonly sourcePinyin: string;
  readonly sourcePinyinSyllables: readonly string[];
  readonly hanzi: string;
  readonly displayPinyin: string;
  readonly displayPinyinSyllables: readonly string[];
  readonly score: number;
}

export interface DocumentRange {
  readonly startOffset: number;
  readonly endOffset: number;
}

export interface AnnotatedSpan {
  readonly id: string;
  readonly kind: 'annotated';
  readonly sourcePinyin: string;
  readonly text: string;
  readonly displayPinyin: string;
  readonly annotationScope: 'character' | 'atomicPhrase';
}

export interface PlainTextSpan {
  readonly id: string;
  readonly kind: 'plain';
  readonly text: string;
}

export type DocumentSpan = AnnotatedSpan | PlainTextSpan;

export interface PhraseToken {
  readonly id: string;
  readonly sourcePinyin: string;
  readonly hanzi: string;
  readonly displayPinyin: string;
}

export interface ComposerDocument {
  readonly schemaVersion: 2;
  readonly id: string;
  readonly title: string;
  readonly spans: readonly DocumentSpan[];
  readonly updatedAtIso: string;
}
