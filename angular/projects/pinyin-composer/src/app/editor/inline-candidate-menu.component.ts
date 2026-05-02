import {
  ChangeDetectionStrategy,
  Component,
  input,
  output,
} from '@angular/core';

import { Candidate } from './phrase-token';

@Component({
  selector: 'app-inline-candidate-menu',
  template: `
    @if (candidates().length > 0) {
      <ol class="candidate-menu" data-testid="candidate-menu">
        @for (
          candidate of candidates();
          track candidate.id;
          let index = $index
        ) {
          <li>
            <button
              type="button"
              data-testid="candidate-option"
              (click)="candidateSelected.emit(candidate)"
            >
              <span class="shortcut">{{ index + 1 }}</span>
              <span class="hanzi">{{ candidate.hanzi }}</span>
              <span class="pinyin">{{ candidate.displayPinyin }}</span>
            </button>
          </li>
        }
      </ol>
    }
  `,
  styles: [
    `
      .candidate-menu {
        display: flex;
        gap: 0.5rem;
        list-style: none;
        padding: 0;
        margin: 0.75rem 0;
      }

      button {
        display: inline-flex;
        align-items: baseline;
        gap: 0.35rem;
        border: 1px solid #cbd5e1;
        border-radius: 999px;
        background: white;
        padding: 0.35rem 0.65rem;
        cursor: pointer;
      }

      .shortcut,
      .pinyin {
        color: #64748b;
        font-size: 0.8rem;
      }
    `,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class InlineCandidateMenuComponent {
  readonly candidates = input.required<readonly Candidate[]>();
  readonly candidateSelected = output<Candidate>();
}
