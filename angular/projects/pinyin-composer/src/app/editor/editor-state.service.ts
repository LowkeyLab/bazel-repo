import { Injectable, computed, signal } from '@angular/core';

import { Candidate, PhraseToken } from './phrase-token';

@Injectable({ providedIn: 'root' })
export class EditorStateService {
  private readonly inputBufferSignal = signal('');
  private readonly tokensSignal = signal<readonly PhraseToken[]>([]);

  readonly inputBuffer = this.inputBufferSignal.asReadonly();
  readonly tokens = this.tokensSignal.asReadonly();
  readonly hasContent = computed(
    () =>
      this.inputBufferSignal().trim().length > 0 ||
      this.tokensSignal().length > 0,
  );

  updateInputBuffer(value: string): void {
    this.inputBufferSignal.set(value);
  }

  loadTokens(tokens: readonly PhraseToken[]): void {
    this.tokensSignal.set([...tokens]);
    this.inputBufferSignal.set('');
  }

  commitCandidate(candidate: Candidate): void {
    this.tokensSignal.update((tokens) => [
      ...tokens,
      this.toToken(candidate, crypto.randomUUID()),
    ]);
    this.inputBufferSignal.set('');
  }

  replaceToken(tokenId: string, candidate: Candidate): void {
    this.tokensSignal.update((tokens) =>
      tokens.map((token) =>
        token.id === tokenId ? this.toToken(candidate, token.id) : token,
      ),
    );
  }

  clear(): void {
    this.inputBufferSignal.set('');
    this.tokensSignal.set([]);
  }

  private toToken(candidate: Candidate, tokenId: string): PhraseToken {
    return {
      id: tokenId,
      sourcePinyin: candidate.sourcePinyin,
      hanzi: candidate.hanzi,
      displayPinyin: candidate.displayPinyin,
    };
  }
}
