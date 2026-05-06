import { TestBed } from '@angular/core/testing';

import { EditorStateService } from './editor-state.service';
import type {
  AnnotatedSpan,
  Candidate,
  ComposerDocument,
  DocumentSpan,
  PlainTextSpan,
} from './phrase-token';

describe('EditorStateService', () => {
  it('inserts plain text into an empty document', () => {
    const service = TestBed.inject(EditorStateService);

    service.replaceRange(0, 0, '你好');

    expect(service.documentText()).toBe('你好');
    expect(service.spans()).toEqual([
      { id: expect.any(String), kind: 'plain', text: '你好' },
    ]);
  });

  it('replaces text at the beginning and end of a document', () => {
    const service = TestBed.inject(EditorStateService);

    service.loadDocument(
      documentWithSpans([plainSpan('plain-1', 'hello world')]),
    );
    service.replaceRange(0, 5, 'hi');
    service.replaceRange(3, 8, 'there');

    expect(service.documentText()).toBe('hi there');
    expect(service.spans()).toEqual([
      { id: expect.any(String), kind: 'plain', text: 'hi there' },
    ]);
  });

  it('expands partial replacement across an atomic phrase annotation', () => {
    const service = TestBed.inject(EditorStateService);

    service.loadDocument(
      documentWithSpans([
        atomicSpan('span-1', 'beijingdaxue', '北京大学', 'Běijīng Dàxué'),
      ]),
    );
    service.replaceRange(1, 3, '平津');

    expect(service.documentText()).toBe('平津');
    expect(service.spans()).toEqual([
      { id: expect.any(String), kind: 'plain', text: '平津' },
    ]);
  });

  it('replaces a range spanning multiple existing spans', () => {
    const service = TestBed.inject(EditorStateService);

    service.loadDocument(
      documentWithSpans([
        plainSpan('plain-1', 'ab'),
        atomicSpan('annotated-1', 'cd', 'CD', 'C D'),
        plainSpan('plain-2', 'ef'),
      ]),
    );
    service.replaceRange(1, 5, 'XYZ');

    expect(service.documentText()).toBe('aXYZf');
    expect(service.spans()).toEqual([
      { id: 'plain-1', kind: 'plain', text: 'aXYZf' },
    ]);
  });

  it('replaces the full document with a single plain span', () => {
    const service = TestBed.inject(EditorStateService);

    service.loadDocument(
      documentWithSpans([
        plainSpan('plain-1', 'abc'),
        atomicSpan('annotated-1', 'de', 'DE', 'D E'),
      ]),
    );
    service.replaceRange(0, service.documentText().length, 'done');

    expect(service.documentText()).toBe('done');
    expect(service.spans()).toEqual([
      { id: expect.any(String), kind: 'plain', text: 'done' },
    ]);
  });

  it('deletes a range with an empty replacement string', () => {
    const service = TestBed.inject(EditorStateService);

    service.loadDocument(
      documentWithSpans([
        plainSpan('plain-1', 'ab'),
        atomicSpan('annotated-1', 'cd', 'CD', 'C D'),
        plainSpan('plain-2', 'ef'),
      ]),
    );
    service.replaceRange(1, 5, '');

    expect(service.documentText()).toBe('af');
    expect(service.spans()).toEqual([
      { id: 'plain-1', kind: 'plain', text: 'af' },
    ]);
  });

  it('merges adjacent plain spans after replacing an atomic phrase annotation', () => {
    const service = TestBed.inject(EditorStateService);

    service.loadDocument(
      documentWithSpans([
        plainSpan('plain-1', 'Hello '),
        atomicSpan('annotated-1', 'shijie', '世界', 'Shìjiè'),
        plainSpan('plain-2', '!'),
      ]),
    );
    service.replaceRange(6, 8, 'world');

    expect(service.documentText()).toBe('Hello world!');
    expect(service.spans()).toEqual([
      { id: 'plain-1', kind: 'plain', text: 'Hello world!' },
    ]);
  });

  it('preserves unaffected annotated span IDs around an edit', () => {
    const service = TestBed.inject(EditorStateService);

    service.loadDocument(
      documentWithSpans([
        characterSpan('annotated-1', 'ni', '你', 'Nǐ'),
        plainSpan('plain-1', ' '),
        characterSpan('annotated-2', 'hao', '好', 'Hǎo'),
      ]),
    );
    service.replaceRange(1, 2, ',');

    expect(service.documentText()).toBe('你,好');
    expect(service.spans()).toEqual([
      characterSpan('annotated-1', 'ni', '你', 'Nǐ'),
      { id: expect.any(String), kind: 'plain', text: ',' },
      characterSpan('annotated-2', 'hao', '好', 'Hǎo'),
    ]);
  });

  it('expands repeatedly across partially intersected atomic phrases', () => {
    const service = TestBed.inject(EditorStateService);

    service.loadDocument(
      documentWithSpans([
        atomicSpan('annotated-1', 'beijing', '北京', 'Běijīng'),
        plainSpan('plain-1', ' and '),
        atomicSpan('annotated-2', 'daxue', '大学', 'Dàxué'),
      ]),
    );
    service.replaceRange(1, 8, '清华');

    expect(service.documentText()).toBe('清华');
    expect(service.spans()).toEqual([
      { id: expect.any(String), kind: 'plain', text: '清华' },
    ]);
  });

  it('keeps exact plain text for punctuation, whitespace, and alphanumerics', () => {
    const service = TestBed.inject(EditorStateService);
    const plainText = '，。！？，.!?\n\tabc123';

    service.replaceRange(0, 0, plainText);

    expect(service.documentText()).toBe(plainText);
    expect(service.spans()).toEqual([
      { id: expect.any(String), kind: 'plain', text: plainText },
    ]);
  });

  it('handles collapsed insertion, boundary replacement, and empty deletion', () => {
    const service = TestBed.inject(EditorStateService);

    service.loadDocument(documentWithSpans([plainSpan('plain-1', 'middle')]));
    service.replaceRange(3, 3, '+');
    expect(service.documentText()).toBe('mid+dle');

    service.loadDocument(
      documentWithSpans([plainSpan('plain-1', 'beginning')]),
    );
    service.replaceRange(0, 5, 'start');
    expect(service.documentText()).toBe('startning');

    service.loadDocument(
      documentWithSpans([plainSpan('plain-1', 'the finish')]),
    );
    service.replaceRange(4, 10, 'end');
    expect(service.documentText()).toBe('the end');

    service.loadDocument(
      documentWithSpans([
        plainSpan('plain-1', 'abc'),
        atomicSpan('annotated-1', 'de', 'DE', 'D E'),
      ]),
    );
    service.replaceRange(0, service.documentText().length, 'full');
    expect(service.documentText()).toBe('full');

    service.loadDocument(
      documentWithSpans([
        plainSpan('plain-1', 'ab'),
        atomicSpan('annotated-1', 'cd', 'CD', 'C D'),
        plainSpan('plain-2', 'ef'),
      ]),
    );
    service.replaceRange(1, 5, '');
    expect(service.documentText()).toBe('af');
  });

  it('commits an aligned candidate to a range as one character span per Hanzi', () => {
    const service = TestBed.inject(EditorStateService);

    service.replaceRange(0, 0, 'beijing');
    service.updateInputBuffer('beijing');
    service.setPendingRange({ startOffset: 0, endOffset: 7 });
    const pendingRange = service.pendingRange();
    expect(pendingRange).toEqual({ startOffset: 0, endOffset: 7 });
    if (!pendingRange) {
      throw new Error('Expected a pending range before committing');
    }
    service.commitCandidateToRange(pendingRange, beijingCandidate());

    expect(service.inputBuffer()).toBe('');
    expect(service.pendingRange()).toBeNull();
    expect(service.documentText()).toBe('北京');
    expect(service.spans()).toEqual([
      {
        id: expect.any(String),
        kind: 'annotated',
        sourcePinyin: 'bei',
        text: '北',
        displayPinyin: 'Běi',
        annotationScope: 'character',
      },
      {
        id: expect.any(String),
        kind: 'annotated',
        sourcePinyin: 'jing',
        text: '京',
        displayPinyin: 'jīng',
        annotationScope: 'character',
      },
    ]);
  });

  it('commits another aligned candidate with per-character pinyin metadata', () => {
    const service = TestBed.inject(EditorStateService);

    service.replaceRange(0, 0, 'zhongguo');
    service.commitCandidateToRange(
      { startOffset: 0, endOffset: 8 },
      zhongguoCandidate(),
    );

    expect(service.documentText()).toBe('中国');
    expect(service.spans()).toEqual([
      {
        id: expect.any(String),
        kind: 'annotated',
        sourcePinyin: 'zhong',
        text: '中',
        displayPinyin: 'Zhōng',
        annotationScope: 'character',
      },
      {
        id: expect.any(String),
        kind: 'annotated',
        sourcePinyin: 'guo',
        text: '国',
        displayPinyin: 'guó',
        annotationScope: 'character',
      },
    ]);
  });

  it('commits an unalignable candidate as one atomic phrase span', () => {
    const service = TestBed.inject(EditorStateService);

    service.replaceRange(0, 0, 'beijing');
    service.commitCandidateToRange(
      { startOffset: 0, endOffset: 7 },
      unalignableCandidate(),
    );

    expect(service.documentText()).toBe('北京A');
    expect(service.spans()).toEqual([
      {
        id: expect.any(String),
        kind: 'annotated',
        sourcePinyin: 'beijing a',
        text: '北京A',
        displayPinyin: 'Běijīng A',
        annotationScope: 'atomicPhrase',
      },
    ]);
  });

  it('deletes and replaces character spans without stale pinyin fragments', () => {
    const service = TestBed.inject(EditorStateService);

    service.loadDocument(documentWithSpans(beijingDaxueCharacterSpans()));
    service.replaceRange(0, 1, '');
    expect(service.spans()).toEqual([
      characterSpan('jing', 'jing', '京', 'jīng'),
      characterSpan('da', 'da', '大', 'dà'),
      characterSpan('xue', 'xue', '学', 'xué'),
    ]);

    service.loadDocument(documentWithSpans(beijingDaxueCharacterSpans()));
    service.replaceRange(1, 2, '');
    expect(service.spans()).toEqual([
      characterSpan('bei', 'bei', '北', 'Běi'),
      characterSpan('da', 'da', '大', 'dà'),
      characterSpan('xue', 'xue', '学', 'xué'),
    ]);

    service.loadDocument(documentWithSpans(beijingDaxueCharacterSpans()));
    service.replaceRange(1, 2, '津');
    expect(service.spans()).toEqual([
      characterSpan('bei', 'bei', '北', 'Běi'),
      { id: expect.any(String), kind: 'plain', text: '津' },
      characterSpan('da', 'da', '大', 'dà'),
      characterSpan('xue', 'xue', '学', 'xué'),
    ]);

    service.loadDocument(documentWithSpans(beijingDaxueCharacterSpans()));
    service.replaceRange(3, 4, '');
    expect(service.spans()).toEqual([
      characterSpan('bei', 'bei', '北', 'Běi'),
      characterSpan('jing', 'jing', '京', 'jīng'),
      characterSpan('da', 'da', '大', 'dà'),
    ]);
  });

  it('expands partial delete and replace inside atomic phrases', () => {
    const service = TestBed.inject(EditorStateService);

    service.loadDocument(
      documentWithSpans([
        plainSpan('plain-1', 'Hi '),
        atomicSpan('phrase-1', 'beijing', '北京', 'Běijīng'),
        plainSpan('plain-2', '!'),
      ]),
    );
    service.replaceRange(4, 5, '');
    expect(service.documentText()).toBe('Hi !');
    expect(service.spans()).toEqual([
      { id: 'plain-1', kind: 'plain', text: 'Hi !' },
    ]);

    service.loadDocument(
      documentWithSpans([
        plainSpan('plain-1', 'Hi '),
        atomicSpan('phrase-1', 'beijing', '北京', 'Běijīng'),
        plainSpan('plain-2', '!'),
      ]),
    );
    service.replaceRange(3, 4, '上海');
    expect(service.documentText()).toBe('Hi 上海!');
    expect(service.spans()).toEqual([
      { id: 'plain-1', kind: 'plain', text: 'Hi 上海!' },
    ]);
  });

  it('does not expand boundary-touching edits around atomic phrases', () => {
    const service = TestBed.inject(EditorStateService);

    service.loadDocument(
      documentWithSpans([
        plainSpan('plain-1', 'A'),
        atomicSpan('phrase-1', 'beijing', '北京', 'Běijīng'),
        plainSpan('plain-2', 'Z'),
      ]),
    );
    service.replaceRange(0, 1, 'B');
    service.replaceRange(3, 4, 'Y');

    expect(service.documentText()).toBe('B北京Y');
    expect(service.spans()).toEqual([
      { id: expect.any(String), kind: 'plain', text: 'B' },
      atomicSpan('phrase-1', 'beijing', '北京', 'Běijīng'),
      { id: expect.any(String), kind: 'plain', text: 'Y' },
    ]);
  });

  it('keeps plain text edits outside annotations unchanged', () => {
    const service = TestBed.inject(EditorStateService);

    service.loadDocument(
      documentWithSpans([
        plainSpan('plain-1', 'abc'),
        characterSpan('bei', 'bei', '北', 'Běi'),
        plainSpan('plain-2', 'xyz'),
      ]),
    );
    service.replaceRange(1, 2, 'Q');
    service.replaceRange(5, 6, 'R');

    expect(service.documentText()).toBe('aQc北xRz');
    expect(service.spans()).toEqual([
      { id: 'plain-1', kind: 'plain', text: 'aQc' },
      characterSpan('bei', 'bei', '北', 'Běi'),
      { id: 'plain-2', kind: 'plain', text: 'xRz' },
    ]);
  });

  it('loads v2 documents and clears transient state', () => {
    const service = TestBed.inject(EditorStateService);
    const document = documentWithSpans([
      plainSpan('plain-1', 'Hi '),
      characterSpan('annotated-1', 'ni', '你', 'Nǐ'),
    ]);

    service.updateInputBuffer('draft');
    service.setPendingRange({ startOffset: 2, endOffset: 1 });
    service.loadDocument(document);

    expect(service.inputBuffer()).toBe('');
    expect(service.pendingRange()).toBeNull();
    expect(service.spans()).toEqual(document.spans);
    expect(service.documentText()).toBe('Hi 你');
  });

  it('reports content from either the input buffer or document text', () => {
    const service = TestBed.inject(EditorStateService);

    expect(service.hasContent()).toBe(false);

    service.updateInputBuffer('   ');
    expect(service.hasContent()).toBe(false);

    service.updateInputBuffer('ni hao');
    expect(service.hasContent()).toBe(true);

    service.updateInputBuffer('');
    service.replaceRange(0, 0, '你好');
    expect(service.hasContent()).toBe(true);
  });

  it('clears spans, input buffer, and pending range', () => {
    const service = TestBed.inject(EditorStateService);

    service.updateInputBuffer('beijing');
    service.setPendingRange({ startOffset: 0, endOffset: 3 });
    service.replaceRange(0, 0, '北京');
    service.clear();

    expect(service.inputBuffer()).toBe('');
    expect(service.pendingRange()).toBeNull();
    expect(service.spans()).toEqual([]);
    expect(service.documentText()).toBe('');
    expect(service.hasContent()).toBe(false);
  });
});

function documentWithSpans(spans: readonly DocumentSpan[]): ComposerDocument {
  return {
    schemaVersion: 2,
    id: 'document-1',
    title: 'Document',
    spans,
    updatedAtIso: '2026-05-06T00:00:00.000Z',
  };
}

function plainSpan(id: string, text: string): PlainTextSpan {
  return {
    id,
    kind: 'plain',
    text,
  };
}

function atomicSpan(
  id: string,
  sourcePinyin: string,
  text: string,
  displayPinyin: string,
): AnnotatedSpan {
  return annotatedSpan(id, sourcePinyin, text, displayPinyin, 'atomicPhrase');
}

function characterSpan(
  id: string,
  sourcePinyin: string,
  text: string,
  displayPinyin: string,
): AnnotatedSpan {
  return annotatedSpan(id, sourcePinyin, text, displayPinyin, 'character');
}

function annotatedSpan(
  id: string,
  sourcePinyin: string,
  text: string,
  displayPinyin: string,
  annotationScope: 'character' | 'atomicPhrase',
): AnnotatedSpan {
  return {
    id,
    kind: 'annotated',
    sourcePinyin,
    text,
    displayPinyin,
    annotationScope,
  };
}

function beijingDaxueCharacterSpans(): readonly AnnotatedSpan[] {
  return [
    characterSpan('bei', 'bei', '北', 'Běi'),
    characterSpan('jing', 'jing', '京', 'jīng'),
    characterSpan('da', 'da', '大', 'dà'),
    characterSpan('xue', 'xue', '学', 'xué'),
  ];
}

function beijingCandidate(): Candidate {
  return {
    id: 'candidate-beijing',
    sourcePinyin: 'beijing',
    sourcePinyinSyllables: ['bei', 'jing'],
    hanzi: '北京',
    displayPinyin: 'Běijīng',
    displayPinyinSyllables: ['Běi', 'jīng'],
    score: 1,
  };
}

function zhongguoCandidate(): Candidate {
  return {
    id: 'candidate-zhongguo',
    sourcePinyin: 'zhongguo',
    sourcePinyinSyllables: ['zhong', 'guo'],
    hanzi: '中国',
    displayPinyin: 'Zhōngguó',
    displayPinyinSyllables: ['Zhōng', 'guó'],
    score: 1,
  };
}

function unalignableCandidate(): Candidate {
  return {
    id: 'candidate-unalignable',
    sourcePinyin: 'beijing a',
    sourcePinyinSyllables: ['bei', 'jing', 'a'],
    hanzi: '北京A',
    displayPinyin: 'Běijīng A',
    displayPinyinSyllables: ['Běi', 'jīng', 'A'],
    score: 1,
  };
}
