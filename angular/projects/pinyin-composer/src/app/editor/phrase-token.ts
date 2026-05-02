export interface Candidate {
  readonly id: string;
  readonly sourcePinyin: string;
  readonly hanzi: string;
  readonly displayPinyin: string;
  readonly score: number;
}

export interface PhraseToken {
  readonly id: string;
  readonly sourcePinyin: string;
  readonly hanzi: string;
  readonly displayPinyin: string;
}

export interface ComposerDocument {
  readonly id: string;
  readonly title: string;
  readonly tokens: readonly PhraseToken[];
  readonly updatedAtIso: string;
}
