import { type ComponentFixture, TestBed } from '@angular/core/testing';

import {
  DocumentEditorComponent,
  type DocumentTextReplacement,
} from './document-editor.component';
import type {
  AnnotatedSpan,
  DocumentSpan,
  PlainTextSpan,
} from './phrase-token';

describe('DocumentEditorComponent', () => {
  let fixture: ComponentFixture<DocumentEditorComponent>;

  beforeEach(async () => {
    await TestBed.configureTestingModule({
      imports: [DocumentEditorComponent],
    }).compileComponents();
    fixture = TestBed.createComponent(DocumentEditorComponent);
  });

  afterEach(() => {
    window.getSelection()?.removeAllRanges();
  });

  it('renders atomic phrase annotated spans as one ruby unit', () => {
    setSpans(fixture, [
      annotatedSpan('annotated-1', 'beijing', '北京', 'Běijīng'),
    ]);

    const annotatedSpans = queryAllByTestId(fixture, 'annotated-span');
    const annotated = queryByTestId(fixture, 'annotated-span');
    const rb = annotated.querySelector('rb') as HTMLElement;
    const rt = annotated.querySelector('rt') as HTMLElement;

    expect(annotatedSpans.length).toBe(1);
    expect(annotated.getAttribute('data-span-id')).toBe('annotated-1');
    expect(rb.textContent).toBe('北京');
    expect(rt.textContent).toBe('Běijīng');
    expect(rt.getAttribute('contenteditable')).toBe('false');
  });

  it('renders adjacent character annotation spans as separate ruby units', () => {
    setSpans(fixture, [
      characterSpan('bei', 'bei', '北', 'Běi'),
      characterSpan('jing', 'jing', '京', 'jīng'),
    ]);

    const annotatedSpans = queryAllByTestId(fixture, 'annotated-span');
    const firstRb = annotatedSpans[0].querySelector('rb') as HTMLElement;
    const firstRt = annotatedSpans[0].querySelector('rt') as HTMLElement;
    const secondRb = annotatedSpans[1].querySelector('rb') as HTMLElement;
    const secondRt = annotatedSpans[1].querySelector('rt') as HTMLElement;

    expect(annotatedSpans.length).toBe(2);
    expect(annotatedSpans[0].getAttribute('data-span-id')).toBe('bei');
    expect(firstRb.textContent).toBe('北');
    expect(firstRt.textContent).toBe('Běi');
    expect(firstRt.getAttribute('contenteditable')).toBe('false');
    expect(annotatedSpans[1].getAttribute('data-span-id')).toBe('jing');
    expect(secondRb.textContent).toBe('京');
    expect(secondRt.textContent).toBe('jīng');
    expect(secondRt.getAttribute('contenteditable')).toBe('false');
  });

  it('renders plain spans inline without rt descendants', () => {
    setSpans(fixture, [plainSpan('plain-1', ' hello ')]);

    const plain = queryByTestId(fixture, 'plain-span');

    expect(plain.getAttribute('data-span-id')).toBe('plain-1');
    expect(plain.textContent).toBe(' hello ');
    expect(plain.querySelector('rt')).toBeNull();
  });

  it('sets the editable textbox attributes on the root', () => {
    fixture.componentRef.setInput('aria-labelledby', 'document-editor-label');
    setSpans(fixture, []);

    const editor = queryEditor(fixture);

    expect(editor.getAttribute('contenteditable')).toBe('true');
    expect(editor.getAttribute('role')).toBe('textbox');
    expect(editor.getAttribute('aria-labelledby')).toBe(
      'document-editor-label',
    );
    expect(editor.getAttribute('aria-multiline')).toBe('true');
  });

  it('maps offsets from base text and excludes rt pinyin text', () => {
    const emitted = collectReplacements(fixture);
    setSpans(fixture, [
      annotatedSpan('annotated-1', 'beijing', '北京', 'Běijīng'),
      plainSpan('plain-1', ' hello '),
    ]);
    const rbText = textNode(queryRequired(fixture, 'rb'));
    const plainText = textNode(queryByTestId(fixture, 'plain-span'));
    selectRange(rbText, 2, plainText, plainText.length);

    dispatchBeforeInput(queryEditor(fixture), 'insertText', 'X');

    expect(emitted).toEqual([{ startOffset: 2, endOffset: 9, text: 'X' }]);
  });

  it('maps offsets across adjacent character ruby spans from base Hanzi only', () => {
    const emitted = collectReplacements(fixture);
    setSpans(fixture, [
      characterSpan('bei', 'bei', '北', 'Běi'),
      characterSpan('jing', 'jing', '京', 'jīng'),
    ]);
    const annotatedSpans = queryAllByTestId(fixture, 'annotated-span');
    const firstRbText = textNode(
      annotatedSpans[0].querySelector('rb') as HTMLElement,
    );
    const secondRbText = textNode(
      annotatedSpans[1].querySelector('rb') as HTMLElement,
    );
    selectRange(firstRbText, 1, secondRbText, 1);

    dispatchBeforeInput(queryEditor(fixture), 'insertText', 'X');

    expect(emitted).toEqual([{ startOffset: 1, endOffset: 2, text: 'X' }]);
  });

  it('maps atomic phrase spans as contiguous base text segments', () => {
    const emitted = collectReplacements(fixture);
    setSpans(fixture, [
      annotatedSpan('annotated-1', 'beijing', '北京', 'Běijīng'),
      plainSpan('plain-1', '!'),
    ]);
    const rbText = textNode(queryRequired(fixture, 'rb'));
    const plainText = textNode(queryByTestId(fixture, 'plain-span'));
    selectRange(rbText, 1, plainText, 0);

    dispatchBeforeInput(queryEditor(fixture), 'deleteContentForward', null);

    expect(emitted).toEqual([{ startOffset: 1, endOffset: 2, text: '' }]);
  });

  it('selects across annotated and plain spans using base text offsets', () => {
    const emitted = collectReplacements(fixture);
    setSpans(fixture, sampleSpans());
    const rbNodes = Array.from(
      fixture.nativeElement.querySelectorAll('rb'),
    ) as HTMLElement[];
    const plainText = textNode(queryByTestId(fixture, 'plain-span'));
    selectRange(textNode(rbNodes[0]), 1, textNode(rbNodes[1]), 1);

    dispatchBeforeInput(queryEditor(fixture), 'deleteContentForward', null);

    expect(plainText.textContent).toBe(' hello ');
    expect(emitted).toEqual([{ startOffset: 1, endOffset: 10, text: '' }]);
  });

  it('emits typed replacement text for typing over a selection', () => {
    const emitted = collectReplacements(fixture);
    setSpans(fixture, sampleSpans());
    const plainText = textNode(queryByTestId(fixture, 'plain-span'));
    selectRange(plainText, 1, plainText, 6);

    dispatchBeforeInput(queryEditor(fixture), 'insertText', 'there');

    expect(emitted).toEqual([{ startOffset: 3, endOffset: 8, text: 'there' }]);
  });

  it('emits typed replacement text for collapsed insertion', () => {
    const emitted = collectReplacements(fixture);
    setSpans(fixture, sampleSpans());
    const plainText = textNode(queryByTestId(fixture, 'plain-span'));
    selectRange(plainText, 3, plainText, 3);

    dispatchBeforeInput(queryEditor(fixture), 'insertText', '!');

    expect(emitted).toEqual([{ startOffset: 5, endOffset: 5, text: '!' }]);
  });

  it('ignores unsupported input events without preventing default', () => {
    const emitted = collectReplacements(fixture);
    setSpans(fixture, [plainSpan('plain-1', 'abc')]);
    const plainText = textNode(queryByTestId(fixture, 'plain-span'));
    selectRange(plainText, 1, plainText, 1);

    const event = dispatchBeforeInput(queryEditor(fixture), 'formatBold', null);

    expect(event.defaultPrevented).toBe(false);
    expect(emitted).toEqual([]);
  });

  it('ignores selections outside the editor', () => {
    const emitted = collectReplacements(fixture);
    const outside = document.createTextNode('outside');
    document.body.appendChild(outside);
    setSpans(fixture, [plainSpan('plain-1', 'abc')]);
    selectRange(outside, 0, outside, outside.length);

    const event = dispatchBeforeInput(queryEditor(fixture), 'insertText', 'X');

    expect(event.defaultPrevented).toBe(false);
    expect(emitted).toEqual([]);
    outside.remove();
  });

  it('emits a newline replacement and prevents paragraph DOM mutation', () => {
    const emitted = collectReplacements(fixture);
    setSpans(fixture, sampleSpans());
    const plainText = textNode(queryByTestId(fixture, 'plain-span'));
    selectRange(plainText, 2, plainText, 2);

    const event = dispatchBeforeInput(
      queryEditor(fixture),
      'insertParagraph',
      null,
    );

    expect(event.defaultPrevented).toBe(true);
    expect(emitted).toEqual([{ startOffset: 4, endOffset: 4, text: '\n' }]);
  });

  it('restores the caret after parent span updates during sequential typing', () => {
    const emitted = collectReplacements(fixture);
    let text = '';
    setSpans(fixture, []);
    selectRange(queryEditor(fixture), 0, queryEditor(fixture), 0);

    for (const character of 'beijing') {
      dispatchBeforeInput(queryEditor(fixture), 'insertText', character);
      const replacement = emitted.at(-1);
      if (!replacement) {
        throw new Error(`Expected replacement for ${character}`);
      }
      text = `${text.slice(0, replacement.startOffset)}${replacement.text}${text.slice(replacement.endOffset)}`;
      setSpans(fixture, [plainSpan('plain-1', text)]);
    }

    expect(emitted).toEqual([
      { startOffset: 0, endOffset: 0, text: 'b' },
      { startOffset: 1, endOffset: 1, text: 'e' },
      { startOffset: 2, endOffset: 2, text: 'i' },
      { startOffset: 3, endOffset: 3, text: 'j' },
      { startOffset: 4, endOffset: 4, text: 'i' },
      { startOffset: 5, endOffset: 5, text: 'n' },
      { startOffset: 6, endOffset: 6, text: 'g' },
    ]);
    expect(queryByTestId(fixture, 'plain-span').textContent).toBe('beijing');
  });

  it('keeps rapid sequential typing ordered before parent render catches up', () => {
    const emitted = collectReplacements(fixture);
    let text = '';
    setSpans(fixture, []);
    selectRange(queryEditor(fixture), 0, queryEditor(fixture), 0);

    for (const character of 'beijing') {
      dispatchBeforeInput(queryEditor(fixture), 'insertText', character);
      const replacement = emitted.at(-1);
      if (!replacement) {
        throw new Error(`Expected replacement for ${character}`);
      }
      text = `${text.slice(0, replacement.startOffset)}${replacement.text}${text.slice(replacement.endOffset)}`;
    }
    setSpans(fixture, [plainSpan('plain-1', text)]);

    expect(emitted).toEqual([
      { startOffset: 0, endOffset: 0, text: 'b' },
      { startOffset: 1, endOffset: 1, text: 'e' },
      { startOffset: 2, endOffset: 2, text: 'i' },
      { startOffset: 3, endOffset: 3, text: 'j' },
      { startOffset: 4, endOffset: 4, text: 'i' },
      { startOffset: 5, endOffset: 5, text: 'n' },
      { startOffset: 6, endOffset: 6, text: 'g' },
    ]);
    expect(queryByTestId(fixture, 'plain-span').textContent).toBe('beijing');
  });

  it('emits an empty replacement for deletion input types', () => {
    const emitted = collectReplacements(fixture);
    setSpans(fixture, sampleSpans());
    const rbText = textNode(queryRequired(fixture, 'rb'));
    selectRange(rbText, 0, rbText, 2);

    dispatchBeforeInput(queryEditor(fixture), 'deleteContentBackward', null);

    expect(emitted).toEqual([{ startOffset: 0, endOffset: 2, text: '' }]);
  });

  it('expands collapsed Backspace to the previous base-text character', () => {
    const emitted = collectReplacements(fixture);
    setSpans(fixture, sampleSpans());
    const plainText = textNode(queryByTestId(fixture, 'plain-span'));
    selectRange(plainText, 0, plainText, 0);

    dispatchBeforeInput(queryEditor(fixture), 'deleteContentBackward', null);

    expect(emitted).toEqual([{ startOffset: 1, endOffset: 2, text: '' }]);
  });

  it('expands collapsed Delete to the next base-text character', () => {
    const emitted = collectReplacements(fixture);
    setSpans(fixture, sampleSpans());
    const plainText = textNode(queryByTestId(fixture, 'plain-span'));
    selectRange(plainText, plainText.length, plainText, plainText.length);

    dispatchBeforeInput(queryEditor(fixture), 'deleteContentForward', null);

    expect(emitted).toEqual([{ startOffset: 9, endOffset: 10, text: '' }]);
  });

  it('expands collapsed Backspace at adjacent character span boundaries', () => {
    const emitted = collectReplacements(fixture);
    setSpans(fixture, [
      characterSpan('bei', 'bei', '北', 'Běi'),
      characterSpan('jing', 'jing', '京', 'jīng'),
    ]);
    const secondRbText = textNode(
      queryAllByTestId(fixture, 'annotated-span')[1].querySelector(
        'rb',
      ) as HTMLElement,
    );
    selectRange(secondRbText, 0, secondRbText, 0);

    dispatchBeforeInput(queryEditor(fixture), 'deleteContentBackward', null);

    expect(emitted).toEqual([{ startOffset: 0, endOffset: 1, text: '' }]);
  });

  it('expands collapsed Delete at adjacent character span boundaries', () => {
    const emitted = collectReplacements(fixture);
    setSpans(fixture, [
      characterSpan('bei', 'bei', '北', 'Běi'),
      characterSpan('jing', 'jing', '京', 'jīng'),
    ]);
    const firstRbText = textNode(
      queryAllByTestId(fixture, 'annotated-span')[0].querySelector(
        'rb',
      ) as HTMLElement,
    );
    selectRange(firstRbText, 1, firstRbText, 1);

    dispatchBeforeInput(queryEditor(fixture), 'deleteContentForward', null);

    expect(emitted).toEqual([{ startOffset: 1, endOffset: 2, text: '' }]);
  });

  it('keeps collapsed Backspace and Delete at document edges in bounds', () => {
    const emitted = collectReplacements(fixture);
    setSpans(fixture, [plainSpan('plain-1', 'ab')]);
    const plainText = textNode(queryByTestId(fixture, 'plain-span'));

    selectRange(plainText, 0, plainText, 0);
    dispatchBeforeInput(queryEditor(fixture), 'deleteContentBackward', null);
    fixture.detectChanges();
    selectRange(plainText, plainText.length, plainText, plainText.length);
    dispatchBeforeInput(queryEditor(fixture), 'deleteContentForward', null);

    expect(emitted).toEqual([
      { startOffset: 0, endOffset: 0, text: '' },
      { startOffset: 2, endOffset: 2, text: '' },
    ]);
  });

  it('emits composed text at the compositionstart range after DOM mutation', () => {
    const emitted = collectReplacements(fixture);
    setSpans(fixture, sampleSpans());
    const plainText = textNode(queryByTestId(fixture, 'plain-span'));
    const editor = queryEditor(fixture);
    selectRange(plainText, 2, plainText, 2);

    editor.dispatchEvent(
      new CompositionEvent('compositionstart', { bubbles: true }),
    );
    const composingInput = dispatchBeforeInput(
      editor,
      'insertCompositionText',
      'n',
    );
    plainText.insertData(2, 'n');
    selectRange(plainText, 3, plainText, 3);
    editor.dispatchEvent(
      new CompositionEvent('compositionend', { bubbles: true, data: '你' }),
    );

    expect(composingInput.defaultPrevented).toBe(false);
    expect(emitted).toEqual([{ startOffset: 4, endOffset: 4, text: '你' }]);
  });

  it('falls back to current selection when compositionend has no captured range', () => {
    const emitted = collectReplacements(fixture);
    setSpans(fixture, sampleSpans());
    const plainText = textNode(queryByTestId(fixture, 'plain-span'));
    const editor = queryEditor(fixture);
    selectRange(plainText, 1, plainText, 1);

    editor.dispatchEvent(
      new CompositionEvent('compositionend', { bubbles: true, data: '好' }),
    );

    expect(emitted).toEqual([{ startOffset: 3, endOffset: 3, text: '好' }]);
  });

  it('ignores compositionend without data or a usable range', () => {
    const emitted = collectReplacements(fixture);
    setSpans(fixture, [plainSpan('plain-1', 'abc')]);
    window.getSelection()?.removeAllRanges();

    queryEditor(fixture).dispatchEvent(
      new CompositionEvent('compositionend', { bubbles: true, data: '' }),
    );
    queryEditor(fixture).dispatchEvent(
      new CompositionEvent('compositionend', { bubbles: true, data: '你' }),
    );

    expect(emitted).toEqual([]);
  });
});

function setSpans(
  fixture: ComponentFixture<DocumentEditorComponent>,
  spans: readonly DocumentSpan[],
): void {
  fixture.componentRef.setInput('spans', spans);
  fixture.detectChanges();
}

function collectReplacements(
  fixture: ComponentFixture<DocumentEditorComponent>,
): DocumentTextReplacement[] {
  const emitted: DocumentTextReplacement[] = [];
  fixture.componentInstance.textReplaced.subscribe((replacement) =>
    emitted.push(replacement),
  );

  return emitted;
}

function sampleSpans(): readonly DocumentSpan[] {
  return [
    annotatedSpan('annotated-1', 'beijing', '北京', 'Běijīng'),
    plainSpan('plain-1', ' hello '),
    annotatedSpan('annotated-2', 'daxue', '大学', 'Dàxué'),
  ];
}

function annotatedSpan(
  id: string,
  sourcePinyin: string,
  text: string,
  displayPinyin: string,
  annotationScope: AnnotatedSpan['annotationScope'] = 'atomicPhrase',
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

function characterSpan(
  id: string,
  sourcePinyin: string,
  text: string,
  displayPinyin: string,
): AnnotatedSpan {
  return annotatedSpan(id, sourcePinyin, text, displayPinyin, 'character');
}

function plainSpan(id: string, text: string): PlainTextSpan {
  return {
    id,
    kind: 'plain',
    text,
  };
}

function queryEditor(
  fixture: ComponentFixture<DocumentEditorComponent>,
): HTMLElement {
  return queryByTestId(fixture, 'document-editor');
}

function queryByTestId(
  fixture: ComponentFixture<DocumentEditorComponent>,
  testId: string,
): HTMLElement {
  return queryRequired(fixture, `[data-testid="${testId}"]`);
}

function queryAllByTestId(
  fixture: ComponentFixture<DocumentEditorComponent>,
  testId: string,
): HTMLElement[] {
  return Array.from(
    fixture.nativeElement.querySelectorAll(`[data-testid="${testId}"]`),
  ) as HTMLElement[];
}

function queryRequired(
  fixture: ComponentFixture<DocumentEditorComponent>,
  selector: string,
): HTMLElement {
  const element = fixture.nativeElement.querySelector(
    selector,
  ) as HTMLElement | null;
  if (!element) {
    throw new Error(`Expected element for selector ${selector}`);
  }

  return element;
}

function textNode(element: HTMLElement): Text {
  const node = element.firstChild;
  if (!(node instanceof Text)) {
    throw new Error('Expected a text node');
  }

  return node;
}

function selectRange(
  startNode: Node,
  startOffset: number,
  endNode: Node,
  endOffset: number,
): void {
  const selection = window.getSelection();
  if (!selection) {
    throw new Error('Expected window selection support');
  }

  const range = document.createRange();
  range.setStart(startNode, startOffset);
  range.setEnd(endNode, endOffset);
  selection.removeAllRanges();
  selection.addRange(range);
}

function dispatchBeforeInput(
  editor: HTMLElement,
  inputType: string,
  data: string | null,
): InputEvent {
  const event = new InputEvent('beforeinput', {
    bubbles: true,
    cancelable: true,
    data,
    inputType,
  });
  editor.dispatchEvent(event);

  return event;
}
