import {
  ChangeDetectionStrategy,
  Component,
  input,
  output,
} from '@angular/core';

import { PhraseToken } from './phrase-token';

@Component({
  selector: 'app-ruby-preview',
  template: `
    <div class="ruby-preview" data-testid="ruby-preview">
      @for (token of tokens(); track token.id) {
        <button
          type="button"
          class="ruby-token-button"
          data-testid="ruby-token"
          (click)="tokenSelected.emit(token.id)"
        >
          <ruby
            ><rb>{{ token.hanzi }}</rb
            ><rt>{{ token.displayPinyin }}</rt></ruby
          >
        </button>
      }
    </div>
  `,
  styles: [
    `
      .ruby-preview {
        display: flex;
        flex-wrap: wrap;
        gap: 0.5rem;
        align-items: flex-end;
        font-size: 2rem;
        line-height: 2.4;
      }

      .ruby-token-button {
        border: 0;
        background: transparent;
        cursor: pointer;
        padding: 0.125rem 0.25rem;
        font: inherit;
      }

      rt {
        font-size: 0.45em;
        color: #475569;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class RubyPreviewComponent {
  readonly tokens = input.required<readonly PhraseToken[]>();
  readonly tokenSelected = output<string>();
}
