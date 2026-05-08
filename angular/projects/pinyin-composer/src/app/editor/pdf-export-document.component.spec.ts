import { type ComponentFixture, TestBed } from '@angular/core/testing';

import { PdfExportDocumentComponent } from './pdf-export-document.component';
import type {
  AnnotatedSpan,
  DocumentSpan,
  PlainTextSpan,
} from './phrase-token';

describe('PdfExportDocumentComponent', () => {
  let fixture: ComponentFixture<PdfExportDocumentComponent>;

  beforeEach(async () => {
    await TestBed.configureTestingModule({
      imports: [PdfExportDocumentComponent],
    }).compileComponents();
    fixture = TestBed.createComponent(PdfExportDocumentComponent);
  });

  it('renders plain text newlines as line breaks without dropping text', () => {
    setSpans(fixture, [plainSpan('plain-1', 'first\n\nsecond')]);

    const root = queryByTestId(fixture, 'pdf-export-document');
    const plainFragments = queryAllByTestId(fixture, 'pdf-export-plain');
    const lineBreaks = queryAllByTestId(fixture, 'pdf-export-line-break');
    const sequence = exportNodes(fixture);

    expect(root.classList.contains('pdf-export-document')).toBe(true);
    expect(plainFragments.map((element) => element.textContent)).toEqual([
      'first',
      'second',
    ]);
    expect(lineBreaks.length).toBe(2);
    expect(sequence).toEqual([
      'pdf-export-plain',
      'pdf-export-line-break',
      'pdf-export-line-break',
      'pdf-export-plain',
    ]);
  });

  it('renders annotated text newlines with ruby fragments and shared pinyin', () => {
    setSpans(fixture, [
      annotatedSpan('annotated-1', 'nihao', '你\n好', 'Nǐ Hǎo'),
    ]);

    const rubyElements = queryAllByTestId(fixture, 'pdf-export-ruby');
    const lineBreaks = queryAllByTestId(fixture, 'pdf-export-line-break');
    const sequence = exportNodes(fixture);

    expect(rubyElements.length).toBe(2);
    expect(rubyElements[0].querySelector('rb')?.textContent).toBe('你');
    expect(rubyElements[0].querySelector('rt')?.textContent).toBe('Nǐ Hǎo');
    expect(rubyElements[1].querySelector('rb')?.textContent).toBe('好');
    expect(rubyElements[1].querySelector('rt')?.textContent).toBe('Nǐ Hǎo');
    expect(lineBreaks.length).toBe(1);
    expect(sequence).toEqual([
      'pdf-export-ruby',
      'pdf-export-line-break',
      'pdf-export-ruby',
    ]);
  });

  it('preserves consecutive, leading, and trailing newlines exactly', () => {
    setSpans(fixture, [
      plainSpan('plain-1', '\nfirst\n\nsecond\n'),
      annotatedSpan('annotated-1', 'hao', '\n好\n', 'Hǎo'),
    ]);

    const lineBreaks = queryAllByTestId(fixture, 'pdf-export-line-break');
    const plainFragments = queryAllByTestId(fixture, 'pdf-export-plain');
    const rubyElements = queryAllByTestId(fixture, 'pdf-export-ruby');

    expect(lineBreaks.length).toBe(6);
    expect(plainFragments.map((element) => element.textContent)).toEqual([
      'first',
      'second',
    ]);
    expect(rubyElements.length).toBe(1);
    expect(rubyElements[0].querySelector('rb')?.textContent).toBe('好');
  });

  it('renders mixed spans in original order and handles empty spans', () => {
    setSpans(fixture, [
      plainSpan('plain-1', 'a\n'),
      annotatedSpan('annotated-1', 'nihao', '你\n好', 'Nǐ Hǎo'),
      plainSpan('plain-2', ''),
    ]);

    expect(exportNodes(fixture)).toEqual([
      'pdf-export-plain',
      'pdf-export-line-break',
      'pdf-export-ruby',
      'pdf-export-line-break',
      'pdf-export-ruby',
    ]);
    expect(queryAllByTestId(fixture, 'pdf-export-plain').length).toBe(1);
  });

  it('renders no fragments for empty spans', () => {
    setSpans(fixture, [plainSpan('plain-1', '')]);

    expect(queryAllByTestId(fixture, 'pdf-export-plain').length).toBe(0);
    expect(queryAllByTestId(fixture, 'pdf-export-ruby').length).toBe(0);
    expect(queryAllByTestId(fixture, 'pdf-export-line-break').length).toBe(0);
  });
});

function setSpans(
  fixture: ComponentFixture<PdfExportDocumentComponent>,
  spans: readonly DocumentSpan[],
): void {
  fixture.componentRef.setInput('spans', spans);
  fixture.detectChanges();
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

function exportNodes(
  fixture: ComponentFixture<PdfExportDocumentComponent>,
): string[] {
  const root = fixture.nativeElement as HTMLElement;
  const elements = Array.from(
    root.querySelectorAll<HTMLElement>(
      '[data-testid="pdf-export-plain"], [data-testid="pdf-export-ruby"], [data-testid="pdf-export-line-break"]',
    ),
  );

  return elements.map((element) => element.getAttribute('data-testid') ?? '');
}

function queryByTestId(
  fixture: ComponentFixture<PdfExportDocumentComponent>,
  testId: string,
): HTMLElement {
  const element = fixture.nativeElement.querySelector(
    `[data-testid="${testId}"]`,
  ) as HTMLElement | null;
  if (!element) {
    throw new Error(`Expected element for ${testId}`);
  }

  return element;
}

function queryAllByTestId(
  fixture: ComponentFixture<PdfExportDocumentComponent>,
  testId: string,
): HTMLElement[] {
  return Array.from(
    fixture.nativeElement.querySelectorAll(`[data-testid="${testId}"]`),
  ) as HTMLElement[];
}
