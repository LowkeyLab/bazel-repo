import { composePinyinTokens } from './pinyin-composer';

describe('composePinyinTokens', () => {
  it('prefers the longest dictionary entry and applies remembered choices', () => {
    const composition = composePinyinTokens(
      ['wo', 'shi', 'zhong', 'guo', 'ren'],
      { shi: '时' },
    );

    expect(composition.selectedText).toBe('我时中国人');
    expect(composition.unknownKeys).toEqual([]);
    expect(
      composition.segments.map((segment) => segment.selectedHanzi),
    ).toEqual(['我', '时', '中国', '人']);
    expect(composition.segments[1]?.isRemembered).toBe(true);
    expect(composition.segments[2]?.pinyin).toBe('zhong guo');
  });

  it('surfaces unknown tokens when no dictionary match exists', () => {
    const composition = composePinyinTokens(['ni', 'mystery'], {});

    expect(composition.unknownKeys).toEqual(['mystery']);
    expect(composition.selectedText).toBe('你mystery');
  });
});
