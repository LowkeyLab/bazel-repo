const DIACRITIC_TO_BASE: Record<string, string> = {
  ā: 'a',
  á: 'a',
  ǎ: 'a',
  à: 'a',
  ē: 'e',
  é: 'e',
  ě: 'e',
  è: 'e',
  ī: 'i',
  í: 'i',
  ǐ: 'i',
  ì: 'i',
  ō: 'o',
  ó: 'o',
  ǒ: 'o',
  ò: 'o',
  ū: 'u',
  ú: 'u',
  ǔ: 'u',
  ù: 'u',
  ǖ: 'ü',
  ǘ: 'ü',
  ǚ: 'ü',
  ǜ: 'ü',
  ü: 'ü',
  ń: 'n',
  ň: 'n',
  ǹ: 'n',
  ḿ: 'm',
};

export interface ParsedPinyinInput {
  readonly tokens: readonly string[];
  readonly normalizedInput: string;
  readonly invalidTokens: readonly string[];
}

export function normalizePinyinToken(token: string): string {
  if (!token.trim()) {
    return '';
  }

  let normalized = token.trim().toLowerCase();
  normalized = normalized.replace(/u:/g, 'ü').replace(/v/g, 'ü');
  normalized = Array.from(normalized, (character) => {
    return DIACRITIC_TO_BASE[character] ?? character;
  }).join('');
  normalized = normalized.replace(/[1-5]/g, '');
  normalized = normalized.replace(/'/g, '');
  normalized = normalized.replace(/[^a-zü]/g, '');

  return /^[a-zü]+$/u.test(normalized) ? normalized : '';
}

export function parsePinyinInput(input: string): ParsedPinyinInput {
  const tokens: string[] = [];
  const invalidTokens: string[] = [];

  for (const rawToken of input.split(/\s+/u).filter(Boolean)) {
    const normalizedToken = normalizePinyinToken(rawToken);

    if (normalizedToken) {
      tokens.push(normalizedToken);
    } else {
      invalidTokens.push(rawToken);
    }
  }

  return {
    tokens,
    normalizedInput: tokens.join(' '),
    invalidTokens,
  };
}
