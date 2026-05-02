import { TestBed } from '@angular/core/testing';

import { HtmlRubyExportService } from './html-ruby-export.service';

describe('HtmlRubyExportService', () => {
  it('exports phrase tokens as semantic ruby html', () => {
    const service = TestBed.inject(HtmlRubyExportService);

    expect(
      service.exportTokens([
        {
          id: 'token-1',
          sourcePinyin: 'beijing',
          hanzi: '北京',
          displayPinyin: 'Běijīng',
        },
      ]),
    ).toBe('<ruby><rb>北京</rb><rt>Běijīng</rt></ruby>');
  });

  it('escapes token text before export', () => {
    const service = TestBed.inject(HtmlRubyExportService);

    expect(
      service.exportTokens([
        {
          id: 'token-1',
          sourcePinyin: 'x',
          hanzi: '<汉>',
          displayPinyin: 'A&B',
        },
      ]),
    ).toBe('<ruby><rb>&lt;汉&gt;</rb><rt>A&amp;B</rt></ruby>');
  });
});
