import { TestBed } from '@angular/core/testing';

import { EditorStateService } from './editor-state.service';
import { Candidate } from './phrase-token';

describe('EditorStateService', () => {
  it('keeps pinyin input separate from committed phrase tokens', () => {
    const service = TestBed.inject(EditorStateService);

    service.updateInputBuffer('wo xiang');

    expect(service.inputBuffer()).toBe('wo xiang');
    expect(service.tokens()).toEqual([]);
  });

  it('commits a selected candidate as a phrase token and clears the input buffer', () => {
    const service = TestBed.inject(EditorStateService);
    const candidate: Candidate = {
      id: 'candidate-0',
      sourcePinyin: 'beijing',
      hanzi: '北京',
      displayPinyin: 'Běijīng',
      score: 1,
    };

    service.updateInputBuffer('beijing');
    service.commitCandidate(candidate);

    expect(service.inputBuffer()).toBe('');
    expect(service.tokens()).toEqual([
      {
        id: expect.any(String),
        sourcePinyin: 'beijing',
        hanzi: '北京',
        displayPinyin: 'Běijīng',
      },
    ]);
  });

  it('replaces a committed token during inline correction', () => {
    const service = TestBed.inject(EditorStateService);

    service.loadTokens([
      { id: 'token-1', sourcePinyin: 'shi', hanzi: '是', displayPinyin: 'Shì' },
    ]);
    service.replaceToken('token-1', {
      id: 'candidate-1',
      sourcePinyin: 'shi',
      hanzi: '时',
      displayPinyin: 'Shí',
      score: 0.8,
    });

    expect(service.tokens()).toEqual([
      { id: 'token-1', sourcePinyin: 'shi', hanzi: '时', displayPinyin: 'Shí' },
    ]);
  });
});
