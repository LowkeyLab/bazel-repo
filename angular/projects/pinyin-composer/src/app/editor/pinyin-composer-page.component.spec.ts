import { type ComponentFixture, TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';

import { LocalDocumentStoreService } from '../documents/local-document-store.service';
import { ConversionWorkerClient } from '../wasm/conversion-worker.client';
import { BrowserPrintService } from './browser-print.service';
import {
  DocumentEditorComponent,
  type DocumentTextReplacement,
} from './document-editor.component';
import { EditorStateService } from './editor-state.service';
import type {
  AnnotatedSpan,
  Candidate,
  ComposerDocument,
  DocumentSpan,
  PlainTextSpan,
} from './phrase-token';
import { PinyinComposerPageComponent } from './pinyin-composer-page.component';

describe('PinyinComposerPageComponent', () => {
  let conversion: FakeConversionWorkerClient;
  let documents: FakeLocalDocumentStoreService;
  let browserPrint: FakeBrowserPrintService;
  let fixture: ComponentFixture<PinyinComposerPageComponent>;
  let editor: EditorStateService;

  beforeEach(async () => {
    conversion = new FakeConversionWorkerClient();
    documents = new FakeLocalDocumentStoreService();
    browserPrint = new FakeBrowserPrintService();

    await TestBed.configureTestingModule({
      imports: [PinyinComposerPageComponent],
      providers: [
        { provide: BrowserPrintService, useValue: browserPrint },
        { provide: ConversionWorkerClient, useValue: conversion },
        { provide: LocalDocumentStoreService, useValue: documents },
      ],
    }).compileComponents();

    fixture = TestBed.createComponent(PinyinComposerPageComponent);
    editor = TestBed.inject(EditorStateService);
    fixture.detectChanges();
  });

  it('renders the document editor controls with the export action', () => {
    const legacyExportTestId = ['html', 'export'].join('-');
    const exportSurface = queryByTestIdRequired(fixture, 'pdf-export-surface');

    expect(queryByTestId(fixture, 'document-editor')).not.toBeNull();
    expect(queryByTestId(fixture, 'document-title')).not.toBeNull();
    expect(queryByTestId(fixture, 'save-document')).not.toBeNull();
    expect(queryByTestId(fixture, 'export-pdf')).not.toBeNull();
    expect(exportSurface.classList.contains('pdf-export-surface')).toBe(true);
    expect(exportSurface.hasAttribute('hidden')).toBe(false);
    expect(exportSurface.hasAttribute('inert')).toBe(true);
    expect(queryByTestId(fixture, 'pinyin-input')).toBeNull();
    expect(queryByTestId(fixture, legacyExportTestId)).toBeNull();
  });

  it('disables export and skips printing when the document is empty', () => {
    const exportButton = queryByTestIdRequired(
      fixture,
      'export-pdf',
    ) as HTMLButtonElement;

    expect(exportButton.disabled).toBe(true);

    exportButton.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    fixture.componentInstance.exportPdf();

    expect(browserPrint.printCallCount).toBe(0);
  });

  it('renders newline-aware PDF content and prints once from the export action', () => {
    fixture.componentInstance.documentTitle.set('Document');
    editor.loadDocument(
      documentWithSpans([
        annotatedSpan('span-bei', 'bei', '北', 'Běi'),
        plainSpan('line-break', '\n'),
        annotatedSpan('span-jing', 'jing', '京', 'jīng'),
      ]),
    );
    fixture.detectChanges();

    const exportButton = queryByTestIdRequired(
      fixture,
      'export-pdf',
    ) as HTMLButtonElement;
    const exportSurface = queryByTestIdRequired(fixture, 'pdf-export-surface');
    const exportSurfaceText = exportSurface.textContent ?? '';
    const exportNodes = exportSurfaceNodes(exportSurface);

    expect(exportButton.disabled).toBe(false);
    expect(exportSurface.classList.contains('pdf-export-surface')).toBe(true);
    expect(exportSurfaceText).not.toContain('Document');
    expect(exportSurfaceText).not.toContain('Untitled pinyin document');
    expect(exportSurfaceText).not.toContain('2026-05-06T00:00:00.000Z');
    expect(exportNodes.map((node) => node.testId)).toEqual([
      'pdf-export-ruby',
      'pdf-export-line-break',
      'pdf-export-ruby',
    ]);
    expect(exportNodes[0].textContent).toContain('北');
    expect(exportNodes[0].textContent).toContain('Běi');
    expect(exportNodes[1].textContent).toBe('');
    expect(exportNodes[2].textContent).toContain('京');
    expect(exportNodes[2].textContent).toContain('jīng');

    clickByTestId(fixture, 'export-pdf');

    expect(browserPrint.printCallCount).toBe(1);
  });

  it('renders consecutive newlines as separate PDF export line breaks', () => {
    editor.loadDocument(
      documentWithSpans([
        annotatedSpan('span-bei', 'bei', '北', 'Běi'),
        plainSpan('blank-line', '\n\n'),
        annotatedSpan('span-jing', 'jing', '京', 'jīng'),
      ]),
    );
    fixture.detectChanges();

    const exportSurface = queryByTestIdRequired(fixture, 'pdf-export-surface');
    const exportNodes = exportSurfaceNodes(exportSurface);

    expect(exportNodes.map((node) => node.testId)).toEqual([
      'pdf-export-ruby',
      'pdf-export-line-break',
      'pdf-export-line-break',
      'pdf-export-ruby',
    ]);
    expect(exportNodes[0].textContent).toContain('北');
    expect(exportNodes[1].textContent).toBe('');
    expect(exportNodes[2].textContent).toBe('');
    expect(exportNodes[3].textContent).toContain('京');
    expect(
      exportSurface.querySelectorAll('[data-testid="pdf-export-line-break"]')
        .length,
    ).toBe(2);
  });

  it('commits the selected candidate into the pending typed range', async () => {
    emitTextReplacement(fixture, {
      startOffset: 0,
      endOffset: 0,
      text: 'beijing',
    });

    expect(conversion.requestSummaries()).toEqual([
      { sourcePinyin: 'beijing', limit: 5 },
    ]);
    conversion.resolveRequest(0, [beijingCandidate()]);
    await settlePromises();
    fixture.detectChanges();

    clickByTestId(fixture, 'candidate-option');
    fixture.detectChanges();

    expect(editor.spans()).toEqual([
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
    expect(editor.inputBuffer()).toBe('');
    expect(editor.pendingRange()).toBeNull();
    expect(fixture.componentInstance.candidates()).toEqual([]);
  });

  it('lets Enter keep its default newline behavior when no candidates exist', () => {
    const event = enterEvent();

    fixture.componentInstance.commitTopCandidate(event);

    expect(event.defaultPrevented).toBe(false);
  });

  it('stores newline replacements as plain document text when no candidates exist', async () => {
    emitTextReplacement(fixture, {
      startOffset: 0,
      endOffset: 0,
      text: '\n',
    });
    await settlePromises();
    fixture.detectChanges();

    expect(editor.documentText()).toBe('\n');
    expect(editor.spans()).toEqual([
      { id: expect.any(String), kind: 'plain', text: '\n' },
    ]);
    expect(conversion.requestSummaries()).toEqual([]);
    expect(fixture.componentInstance.candidates()).toEqual([]);
  });

  it('accumulates collapsed sequential insertions into one active pinyin run', () => {
    emitSequentialText(fixture, 'bei');

    expect(conversion.requestSummaries()).toEqual([
      { sourcePinyin: 'b', limit: 5 },
      { sourcePinyin: 'be', limit: 5 },
      { sourcePinyin: 'bei', limit: 5 },
    ]);
    expect(editor.pendingRange()).toEqual({ startOffset: 0, endOffset: 3 });
    expect(editor.inputBuffer()).toBe('bei');
  });

  it('keeps conversion candidates synced after deleting inside the active pinyin run', () => {
    emitSequentialText(fixture, 'beijing');

    emitTextReplacement(fixture, {
      startOffset: 3,
      endOffset: 4,
      text: '',
    });

    expect(editor.documentText()).toBe('beiing');
    expect(editor.pendingRange()).toEqual({ startOffset: 0, endOffset: 6 });
    expect(editor.inputBuffer()).toBe('beiing');
    expect(conversion.requestSummaries().at(-1)).toEqual({
      sourcePinyin: 'beiing',
      limit: 5,
    });
  });

  it('uses accumulated spaced pinyin and commits over the full pending run', async () => {
    emitSequentialText(fixture, 'bei jing');

    expect(conversion.requestSummaries()).toEqual([
      { sourcePinyin: 'b', limit: 5 },
      { sourcePinyin: 'be', limit: 5 },
      { sourcePinyin: 'bei', limit: 5 },
      { sourcePinyin: 'bei j', limit: 5 },
      { sourcePinyin: 'bei ji', limit: 5 },
      { sourcePinyin: 'bei jin', limit: 5 },
      { sourcePinyin: 'bei jing', limit: 5 },
    ]);
    expect(editor.pendingRange()).toEqual({ startOffset: 0, endOffset: 8 });
    expect(editor.inputBuffer()).toBe('bei jing');

    conversion.resolveRequest(6, [spacedBeijingCandidate()]);
    await settlePromises();
    fixture.detectChanges();

    clickByTestId(fixture, 'candidate-option');
    fixture.detectChanges();

    expect(editor.spans()).toEqual([
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
    expect(editor.inputBuffer()).toBe('');
    expect(editor.pendingRange()).toBeNull();
  });

  it('handles normal document editor key events with an internal space', async () => {
    const documentEditor = queryByTestIdRequired(fixture, 'document-editor');
    selectRange(documentEditor, 0, documentEditor, 0);

    for (const character of 'bei jing') {
      const event = dispatchBeforeInput(
        documentEditor,
        'insertText',
        character,
      );
      fixture.detectChanges();

      expect(event.defaultPrevented).toBe(true);
    }

    expect(editor.documentText()).toBe('bei jing');
    expect(conversion.requestSummaries()).toEqual([
      { sourcePinyin: 'b', limit: 5 },
      { sourcePinyin: 'be', limit: 5 },
      { sourcePinyin: 'bei', limit: 5 },
      { sourcePinyin: 'bei j', limit: 5 },
      { sourcePinyin: 'bei ji', limit: 5 },
      { sourcePinyin: 'bei jin', limit: 5 },
      { sourcePinyin: 'bei jing', limit: 5 },
    ]);
    expect(editor.pendingRange()).toEqual({ startOffset: 0, endOffset: 8 });
    expect(editor.inputBuffer()).toBe('bei jing');

    conversion.resolveRequest(6, [spacedBeijingCandidate()]);
    await settlePromises();
    fixture.detectChanges();

    expect(candidateMenuText(fixture)).toContain('北京');
    expect(
      fixture.componentInstance.candidates().map((candidate) => candidate.id),
    ).toEqual(['candidate-bei-jing']);
  });

  it('keeps delayed shorter grouped responses from replacing newer candidates', async () => {
    emitSequentialText(fixture, 'bei jing');

    conversion.resolveRequest(6, [spacedBeijingCandidate()]);
    await settlePromises();
    fixture.detectChanges();
    expect(candidateMenuText(fixture)).toContain('北京');

    conversion.resolveRequest(1, [beCandidate()]);
    await settlePromises();
    fixture.detectChanges();

    expect(candidateMenuText(fixture)).toContain('北京');
    expect(
      fixture.componentInstance.candidates().map((candidate) => candidate.id),
    ).toEqual(['candidate-bei-jing']);
  });

  it('clears candidates without conversion for newline and whitespace-only replacements', async () => {
    fixture.componentInstance.candidates.set([beijingCandidate()]);

    emitTextReplacement(fixture, {
      startOffset: 0,
      endOffset: 0,
      text: '\n',
    });
    await settlePromises();
    fixture.detectChanges();

    expect(conversion.requestSummaries()).toEqual([]);
    expect(fixture.componentInstance.candidates()).toEqual([]);
    expect(editor.pendingRange()).toBeNull();
    expect(editor.inputBuffer()).toBe('');

    fixture.componentInstance.candidates.set([beijingCandidate()]);
    emitTextReplacement(fixture, {
      startOffset: 1,
      endOffset: 1,
      text: '   ',
    });
    await settlePromises();
    fixture.detectChanges();

    expect(editor.documentText()).toBe('\n   ');
    expect(conversion.requestSummaries()).toEqual([]);
    expect(fixture.componentInstance.candidates()).toEqual([]);
    expect(editor.pendingRange()).toBeNull();
    expect(editor.inputBuffer()).toBe('');
  });

  it('prevents Enter and commits the top candidate when a pending range exists', () => {
    editor.replaceRange(0, 0, 'beijing');
    editor.setPendingRange({ startOffset: 0, endOffset: 7 });
    editor.updateInputBuffer('beijing');
    fixture.componentInstance.candidates.set([beijingCandidate()]);
    const event = enterEvent();

    fixture.componentInstance.commitTopCandidate(event);

    expect(event.defaultPrevented).toBe(true);
    expect(editor.spans()).toEqual([
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
    expect(editor.inputBuffer()).toBe('');
    expect(editor.pendingRange()).toBeNull();
    expect(fixture.componentInstance.candidates()).toEqual([]);
  });

  it('keeps uncommitted replacement text as a plain inline span', async () => {
    emitTextReplacement(fixture, {
      startOffset: 0,
      endOffset: 0,
      text: ' hello, 123! ',
    });

    expect(conversion.requestSummaries()).toEqual([
      { sourcePinyin: 'hello, 123!', limit: 5 },
    ]);
    conversion.resolveRequest(0, []);
    await settlePromises();
    fixture.detectChanges();

    expect(editor.spans()).toEqual([
      { id: expect.any(String), kind: 'plain', text: ' hello, 123! ' },
    ]);
    expect(queryByTestId(fixture, 'plain-span')?.textContent).toBe(
      ' hello, 123! ',
    );
    expect(queryByTestId(fixture, 'annotated-span')).toBeNull();
  });

  it('does not let stale conversion responses replace newer candidates', async () => {
    emitTextReplacement(fixture, { startOffset: 0, endOffset: 0, text: 'bei' });
    emitTextReplacement(fixture, {
      startOffset: 0,
      endOffset: 3,
      text: 'beijing',
    });

    expect(conversion.requestSummaries()).toEqual([
      { sourcePinyin: 'bei', limit: 5 },
      { sourcePinyin: 'beijing', limit: 5 },
    ]);

    conversion.resolveRequest(1, [beijingCandidate()]);
    await settlePromises();
    fixture.detectChanges();
    expect(candidateMenuText(fixture)).toContain('北京');

    conversion.resolveRequest(0, [olderCandidate()]);
    await settlePromises();
    fixture.detectChanges();

    expect(candidateMenuText(fixture)).toContain('北京');
    expect(
      fixture.componentInstance.candidates().map((candidate) => candidate.id),
    ).toEqual(['candidate-beijing']);
  });

  it('converts only replaced text and preserves spans outside the range', async () => {
    editor.loadDocument(
      documentWithSpans([
        annotatedSpan('annotated-1', 'ni', '你', 'Nǐ'),
        plainSpan('plain-1', ' old '),
        annotatedSpan('annotated-2', 'hao', '好', 'Hǎo'),
      ]),
    );
    const initialSpanIds = editor.spans().map((span) => span.id);

    emitTextReplacement(fixture, {
      startOffset: 1,
      endOffset: 6,
      text: 'beijing',
    });

    expect(editor.documentText()).toBe('你beijing好');
    expect(editor.spans().at(0)?.id).toBe(initialSpanIds[0]);
    expect(editor.spans().at(-1)?.id).toBe(initialSpanIds[2]);
    expect(conversion.requestSummaries()).toEqual([
      { sourcePinyin: 'beijing', limit: 5 },
    ]);

    conversion.resolveRequest(0, [beijingCandidate()]);
    await settlePromises();
    fixture.detectChanges();

    clickByTestId(fixture, 'candidate-option');
    fixture.detectChanges();

    expect(editor.spans()).toEqual([
      annotatedSpan('annotated-1', 'ni', '你', 'Nǐ'),
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
      annotatedSpan('annotated-2', 'hao', '好', 'Hǎo'),
    ]);
    expect(conversion.requestSummaries()).not.toContain({
      sourcePinyin: '你beijing好',
      limit: 5,
    });
  });

  it('tracks expanded atomic phrase replacements as the pending commit range', async () => {
    editor.loadDocument(
      documentWithSpans([atomicSpan('phrase-1', 'beijing', '北京', 'Běijīng')]),
    );

    emitTextReplacement(fixture, {
      startOffset: 1,
      endOffset: 2,
      text: 'shanghai',
    });

    expect(editor.documentText()).toBe('shanghai');
    expect(editor.pendingRange()).toEqual({ startOffset: 0, endOffset: 8 });
    expect(conversion.requestSummaries()).toEqual([
      { sourcePinyin: 'shanghai', limit: 5 },
    ]);

    conversion.resolveRequest(0, [shanghaiCandidate()]);
    await settlePromises();
    fixture.detectChanges();

    clickByTestId(fixture, 'candidate-option');
    fixture.detectChanges();

    expect(editor.spans()).toEqual([
      {
        id: expect.any(String),
        kind: 'annotated',
        sourcePinyin: 'shang',
        text: '上',
        displayPinyin: 'Shàng',
        annotationScope: 'character',
      },
      {
        id: expect.any(String),
        kind: 'annotated',
        sourcePinyin: 'hai',
        text: '海',
        displayPinyin: 'hǎi',
        annotationScope: 'character',
      },
    ]);
  });
});

interface ConversionRequest {
  readonly sourcePinyin: string;
  readonly limit: number;
  readonly deferred: Deferred<readonly Candidate[]>;
}

interface Deferred<T> {
  readonly promise: Promise<T>;
  readonly resolve: (value: T) => void;
  readonly reject: (reason: unknown) => void;
}

class FakeConversionWorkerClient implements Pick<
  ConversionWorkerClient,
  'convertPinyin'
> {
  private readonly requests: ConversionRequest[] = [];

  convertPinyin(
    sourcePinyin: string,
    limit: number,
  ): Promise<readonly Candidate[]> {
    const deferred = createDeferred<readonly Candidate[]>();
    this.requests.push({ sourcePinyin, limit, deferred });

    return deferred.promise;
  }

  requestSummaries(): {
    readonly sourcePinyin: string;
    readonly limit: number;
  }[] {
    return this.requests.map(({ sourcePinyin, limit }) => ({
      sourcePinyin,
      limit,
    }));
  }

  resolveRequest(index: number, candidates: readonly Candidate[]): void {
    const request = this.requests[index];
    if (!request) {
      throw new Error(`Expected conversion request at index ${index}`);
    }
    request.deferred.resolve(candidates);
  }
}

class FakeLocalDocumentStoreService implements Pick<
  LocalDocumentStoreService,
  'saveDocument'
> {
  readonly savedDocuments: ComposerDocument[] = [];

  saveDocument(document: ComposerDocument): void {
    this.savedDocuments.push(document);
  }
}

class FakeBrowserPrintService implements Pick<BrowserPrintService, 'print'> {
  printCallCount = 0;

  print(): void {
    this.printCallCount += 1;
  }
}

function documentWithSpans(spans: readonly DocumentSpan[]): ComposerDocument {
  return {
    schemaVersion: 2,
    id: 'document-1',
    title: 'Document',
    spans,
    updatedAtIso: '2026-05-06T00:00:00.000Z',
  };
}

function annotatedSpan(
  id: string,
  sourcePinyin: string,
  text: string,
  displayPinyin: string,
): AnnotatedSpan {
  return {
    id,
    kind: 'annotated',
    sourcePinyin,
    text,
    displayPinyin,
    annotationScope: text.length === 1 ? 'character' : 'atomicPhrase',
  };
}

function atomicSpan(
  id: string,
  sourcePinyin: string,
  text: string,
  displayPinyin: string,
): AnnotatedSpan {
  return {
    id,
    kind: 'annotated',
    sourcePinyin,
    text,
    displayPinyin,
    annotationScope: 'atomicPhrase',
  };
}

function plainSpan(id: string, text: string): PlainTextSpan {
  return {
    id,
    kind: 'plain',
    text,
  };
}

function createDeferred<T>(): Deferred<T> {
  let resolve: (value: T) => void = () => undefined;
  let reject: (reason: unknown) => void = () => undefined;
  const promise = new Promise<T>((promiseResolve, promiseReject) => {
    resolve = promiseResolve;
    reject = promiseReject;
  });

  return { promise, resolve, reject };
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

function olderCandidate(): Candidate {
  return {
    id: 'candidate-bei',
    sourcePinyin: 'bei',
    sourcePinyinSyllables: ['bei'],
    hanzi: '北',
    displayPinyin: 'Běi',
    displayPinyinSyllables: ['Běi'],
    score: 1,
  };
}

function spacedBeijingCandidate(): Candidate {
  return {
    id: 'candidate-bei-jing',
    sourcePinyin: 'bei jing',
    sourcePinyinSyllables: ['bei', 'jing'],
    hanzi: '北京',
    displayPinyin: 'Běijīng',
    displayPinyinSyllables: ['Běi', 'jīng'],
    score: 1,
  };
}

function beCandidate(): Candidate {
  return {
    id: 'candidate-be',
    sourcePinyin: 'be',
    sourcePinyinSyllables: ['be'],
    hanzi: '贝',
    displayPinyin: 'Bèi',
    displayPinyinSyllables: ['Bèi'],
    score: 1,
  };
}

function shanghaiCandidate(): Candidate {
  return {
    id: 'candidate-shanghai',
    sourcePinyin: 'shanghai',
    sourcePinyinSyllables: ['shang', 'hai'],
    hanzi: '上海',
    displayPinyin: 'Shànghǎi',
    displayPinyinSyllables: ['Shàng', 'hǎi'],
    score: 1,
  };
}

function emitSequentialText(
  fixture: ComponentFixture<PinyinComposerPageComponent>,
  text: string,
): void {
  let offset = 0;
  for (const character of text) {
    emitTextReplacement(fixture, {
      startOffset: offset,
      endOffset: offset,
      text: character,
    });
    offset += character.length;
  }
}

function emitTextReplacement(
  fixture: ComponentFixture<PinyinComposerPageComponent>,
  replacement: DocumentTextReplacement,
): void {
  const documentEditor = fixture.debugElement.query(
    By.directive(DocumentEditorComponent),
  )?.componentInstance as DocumentEditorComponent | undefined;
  if (!documentEditor) {
    throw new Error('Expected DocumentEditorComponent in page fixture');
  }
  documentEditor.textReplaced.emit(replacement);
}

function dispatchBeforeInput(
  element: HTMLElement,
  inputType: string,
  data: string | null,
): InputEvent {
  const event = new InputEvent('beforeinput', {
    bubbles: true,
    cancelable: true,
    data,
    inputType,
  });
  element.dispatchEvent(event);

  return event;
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

function queryByTestIdRequired(
  fixture: ComponentFixture<PinyinComposerPageComponent>,
  testId: string,
): HTMLElement {
  const element = queryByTestId(fixture, testId);
  if (!element) {
    throw new Error(`Expected element for ${testId}`);
  }

  return element;
}

function clickByTestId(
  fixture: ComponentFixture<PinyinComposerPageComponent>,
  testId: string,
): void {
  const element = queryByTestId(fixture, testId);
  if (!(element instanceof HTMLElement)) {
    throw new Error(`Expected clickable element for ${testId}`);
  }
  element.click();
}

interface ExportSurfaceNode {
  readonly testId: string;
  readonly textContent: string;
}

function exportSurfaceNodes(exportSurface: HTMLElement): ExportSurfaceNode[] {
  return Array.from(
    exportSurface.querySelectorAll<HTMLElement>(
      '[data-testid="pdf-export-plain"], [data-testid="pdf-export-ruby"], [data-testid="pdf-export-line-break"]',
    ),
    (element) => ({
      testId: element.getAttribute('data-testid') ?? '',
      textContent: element.textContent ?? '',
    }),
  );
}

function candidateMenuText(
  fixture: ComponentFixture<PinyinComposerPageComponent>,
): string {
  return queryByTestId(fixture, 'candidate-menu')?.textContent ?? '';
}

function enterEvent(): KeyboardEvent {
  return new KeyboardEvent('keydown', {
    cancelable: true,
    key: 'Enter',
  });
}

function queryByTestId(
  fixture: ComponentFixture<PinyinComposerPageComponent>,
  testId: string,
): HTMLElement | null {
  return fixture.nativeElement.querySelector(
    `[data-testid="${testId}"]`,
  ) as HTMLElement | null;
}

async function settlePromises(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
}
