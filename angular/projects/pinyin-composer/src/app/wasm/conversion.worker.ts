import initWasm, {
  convert_pinyin_js,
} from './generated/pinyin_composer_wasm.js';

interface ConvertPinyinRequest {
  readonly requestId: string;
  readonly type: 'convert-pinyin';
  readonly sourcePinyin: string;
  readonly limit: number;
}

interface WorkerCandidate {
  readonly id: string;
  readonly sourcePinyin: string;
  readonly sourcePinyinSyllables: readonly string[];
  readonly hanzi: string;
  readonly displayPinyin: string;
  readonly displayPinyinSyllables: readonly string[];
  readonly score: number;
}

interface ConversionResult {
  readonly sourcePinyin: string;
  readonly candidates: readonly WorkerCandidate[];
}

const wasmReady = initWasm(
  new URL('/generated/pinyin_composer_wasm_bg.wasm', self.location.origin),
);

self.onmessage = (event: MessageEvent<ConvertPinyinRequest>) => {
  if (event.origin && event.origin !== self.location.origin) {
    return;
  }

  void handleMessage(event.data);
};

async function handleMessage(request: ConvertPinyinRequest): Promise<void> {
  try {
    await wasmReady;
    const result = convertInWorker(request.sourcePinyin, request.limit);
    self.postMessage({
      requestId: request.requestId,
      type: 'conversion-result',
      result,
    });
  } catch (error: unknown) {
    self.postMessage({
      requestId: request.requestId,
      type: 'conversion-error',
      message: error instanceof Error ? error.message : String(error),
    });
  }
}

function convertInWorker(
  sourcePinyin: string,
  limit: number,
): ConversionResult {
  const normalized = normalize(sourcePinyin);
  if (!normalized) {
    return { sourcePinyin: normalized, candidates: [] };
  }

  const rawResult: unknown = convert_pinyin_js(normalized, limit);
  return parseConversionResult(rawResult);
}

function normalize(sourcePinyin: string): string {
  return sourcePinyin
    .trim()
    .split(/\s+/)
    .filter(Boolean)
    .join(' ')
    .toLowerCase();
}

function parseConversionResult(value: unknown): ConversionResult {
  const result = expectRecord(value, 'conversion result');
  const { candidates: rawCandidates, sourcePinyin: rawSourcePinyin } = result;
  const sourcePinyin = expectString(rawSourcePinyin, 'sourcePinyin');
  if (!Array.isArray(rawCandidates)) {
    throw new Error('conversion result candidates must be an array');
  }

  return {
    sourcePinyin,
    candidates: rawCandidates.map(parseCandidate),
  };
}

function parseCandidate(value: unknown): WorkerCandidate {
  const candidate = expectRecord(value, 'candidate');
  const {
    displayPinyin,
    displayPinyinSyllables,
    hanzi,
    id,
    score,
    sourcePinyin,
    sourcePinyinSyllables,
  } = candidate;
  return {
    id: expectString(id, 'candidate.id'),
    sourcePinyin: expectString(sourcePinyin, 'candidate.sourcePinyin'),
    sourcePinyinSyllables: expectStringArray(
      sourcePinyinSyllables,
      'candidate.sourcePinyinSyllables',
    ),
    hanzi: expectString(hanzi, 'candidate.hanzi'),
    displayPinyin: expectString(displayPinyin, 'candidate.displayPinyin'),
    displayPinyinSyllables: expectStringArray(
      displayPinyinSyllables,
      'candidate.displayPinyinSyllables',
    ),
    score: expectNumber(score, 'candidate.score'),
  };
}

function expectRecord(
  value: unknown,
  fieldName: string,
): Record<string, unknown> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    throw new Error(`${fieldName} must be an object`);
  }
  return value as Record<string, unknown>;
}

function expectString(value: unknown, fieldName: string): string {
  if (typeof value !== 'string') {
    throw new Error(`${fieldName} must be a string`);
  }
  return value;
}

function expectStringArray(
  value: unknown,
  fieldName: string,
): readonly string[] {
  if (
    !Array.isArray(value) ||
    !value.every((entry) => typeof entry === 'string')
  ) {
    throw new Error(`${fieldName} must be an array of strings`);
  }
  return value;
}

function expectNumber(value: unknown, fieldName: string): number {
  if (typeof value !== 'number') {
    throw new Error(`${fieldName} must be a number`);
  }
  return value;
}
