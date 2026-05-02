import { Injectable } from '@angular/core';

import { PhraseToken } from '../editor/phrase-token';

@Injectable({ providedIn: 'root' })
export class HtmlRubyExportService {
  exportTokens(tokens: readonly PhraseToken[]): string {
    return tokens
      .map(
        (token) =>
          `<ruby><rb>${this.escapeHtml(token.hanzi)}</rb><rt>${this.escapeHtml(token.displayPinyin)}</rt></ruby>`,
      )
      .join('');
  }

  private escapeHtml(value: string): string {
    return value
      .replaceAll('&', '&amp;')
      .replaceAll('<', '&lt;')
      .replaceAll('>', '&gt;')
      .replaceAll('"', '&quot;')
      .replaceAll("'", '&#39;');
  }
}
