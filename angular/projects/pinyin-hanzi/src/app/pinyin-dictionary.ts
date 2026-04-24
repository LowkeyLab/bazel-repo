export interface HanziOption {
  readonly hanzi: string;
  readonly gloss: string;
}

export interface PinyinDictionaryEntry {
  readonly key: string;
  readonly syllables: readonly string[];
  readonly options: readonly HanziOption[];
}

function defineEntry(
  key: string,
  options: readonly HanziOption[],
): PinyinDictionaryEntry {
  return {
    key,
    syllables: key.split(' '),
    options,
  };
}

export const PINYIN_DICTIONARY: readonly PinyinDictionaryEntry[] = [
  defineEntry('ni', [{ hanzi: '你', gloss: 'you' }]),
  defineEntry('hao', [{ hanzi: '好', gloss: 'good' }]),
  defineEntry('ni hao', [{ hanzi: '你好', gloss: 'hello' }]),
  defineEntry('ma', [
    { hanzi: '吗', gloss: 'question particle' },
    { hanzi: '妈', gloss: 'mother' },
    { hanzi: '马', gloss: 'horse' },
  ]),
  defineEntry('wo', [{ hanzi: '我', gloss: 'I / me' }]),
  defineEntry('shi', [
    { hanzi: '是', gloss: 'to be' },
    { hanzi: '时', gloss: 'time' },
    { hanzi: '事', gloss: 'matter' },
  ]),
  defineEntry('zhong guo', [{ hanzi: '中国', gloss: 'China' }]),
  defineEntry('ren', [{ hanzi: '人', gloss: 'person' }]),
  defineEntry('xue sheng', [{ hanzi: '学生', gloss: 'student' }]),
  defineEntry('peng you', [{ hanzi: '朋友', gloss: 'friend' }]),
  defineEntry('ai', [{ hanzi: '爱', gloss: 'love' }]),
  defineEntry('he cha', [{ hanzi: '喝茶', gloss: 'drink tea' }]),
  defineEntry('chi fan', [{ hanzi: '吃饭', gloss: 'eat a meal' }]),
];

export const PINYIN_DICTIONARY_BY_KEY = new Map(
  PINYIN_DICTIONARY.map((entry) => [entry.key, entry]),
);

export const MAX_PINYIN_ENTRY_LENGTH = PINYIN_DICTIONARY.reduce(
  (maxLength, entry) => Math.max(maxLength, entry.syllables.length),
  1,
);
