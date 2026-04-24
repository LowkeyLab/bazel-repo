import { parsePinyinInput } from './pinyin-parser';

describe('parsePinyinInput', () => {
  it('normalizes spacing, tone marks, and tone numbers', () => {
    expect(parsePinyinInput('  Nǐ   hao3   ma5 ')).toEqual({
      tokens: ['ni', 'hao', 'ma'],
      normalizedInput: 'ni hao ma',
      invalidTokens: [],
    });
  });

  it('keeps invalid tokens separate from valid ones', () => {
    expect(parsePinyinInput('wo @@@ pengyou!')).toEqual({
      tokens: ['wo', 'pengyou'],
      normalizedInput: 'wo pengyou',
      invalidTokens: ['@@@'],
    });
  });
});
