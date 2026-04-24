import {
  MAX_PINYIN_ENTRY_LENGTH,
  PINYIN_DICTIONARY_BY_KEY,
} from './pinyin-dictionary';
import type {
  HanziOption,
  PinyinDictionaryEntry,
} from './pinyin-dictionary';
import type { RememberedChoices } from './remembered-choices.store';

export interface ComposedSegment {
  readonly key: string;
  readonly pinyin: string;
  readonly options: readonly HanziOption[];
  readonly selectedHanzi: string | null;
  readonly isAmbiguous: boolean;
  readonly isKnown: boolean;
  readonly isRemembered: boolean;
}

export interface CompositionResult {
  readonly segments: readonly ComposedSegment[];
  readonly unknownKeys: readonly string[];
  readonly selectedText: string;
}

function pickRememberedOption(
  entry: PinyinDictionaryEntry,
  rememberedChoices: RememberedChoices,
): { selectedHanzi: string; isRemembered: boolean } {
  const rememberedHanzi = rememberedChoices[entry.key];
  const selectedOption = entry.options.find(
    (option) => option.hanzi === rememberedHanzi,
  );

  if (selectedOption) {
    return {
      selectedHanzi: selectedOption.hanzi,
      isRemembered: true,
    };
  }

  return {
    selectedHanzi: entry.options[0]?.hanzi ?? '',
    isRemembered: false,
  };
}

export function composePinyinTokens(
  tokens: readonly string[],
  rememberedChoices: RememberedChoices,
): CompositionResult {
  const segments: ComposedSegment[] = [];
  const unknownKeys: string[] = [];
  const selectedText: string[] = [];

  let index = 0;

  while (index < tokens.length) {
    const remainingLength = Math.min(
      MAX_PINYIN_ENTRY_LENGTH,
      tokens.length - index,
    );

    let matchedEntry: PinyinDictionaryEntry | undefined;

    for (let length = remainingLength; length > 0; length -= 1) {
      const candidateKey = tokens.slice(index, index + length).join(' ');
      const entry = PINYIN_DICTIONARY_BY_KEY.get(candidateKey);

      if (entry) {
        matchedEntry = entry;
        break;
      }
    }

    if (!matchedEntry) {
      const unknownKey = tokens[index] ?? '';

      segments.push({
        key: unknownKey,
        pinyin: unknownKey,
        options: [],
        selectedHanzi: null,
        isAmbiguous: false,
        isKnown: false,
        isRemembered: false,
      });
      unknownKeys.push(unknownKey);
      selectedText.push(unknownKey);
      index += 1;
      continue;
    }

    const selectedOption = pickRememberedOption(
      matchedEntry,
      rememberedChoices,
    );

    segments.push({
      key: matchedEntry.key,
      pinyin: matchedEntry.key,
      options: matchedEntry.options,
      selectedHanzi: selectedOption.selectedHanzi,
      isAmbiguous: matchedEntry.options.length > 1,
      isKnown: true,
      isRemembered: selectedOption.isRemembered,
    });
    selectedText.push(selectedOption.selectedHanzi);
    index += matchedEntry.syllables.length;
  }

  return {
    segments,
    unknownKeys,
    selectedText: selectedText.join(''),
  };
}
