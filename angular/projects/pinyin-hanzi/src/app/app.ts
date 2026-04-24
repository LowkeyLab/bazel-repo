import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  signal,
} from '@angular/core';

import { composePinyinTokens } from './pinyin-composer';
import type { ComposedSegment } from './pinyin-composer';
import { PINYIN_DICTIONARY } from './pinyin-dictionary';
import { parsePinyinInput } from './pinyin-parser';
import { RememberedChoicesStore } from './remembered-choices.store';

@Component({
  selector: 'app-root',
  imports: [],
  templateUrl: './app.html',
  styleUrl: './app.css',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class App {
  private readonly rememberedChoicesStore = inject(RememberedChoicesStore);

  protected readonly inputValue = signal('ni hao ma');
  protected readonly dictionaryEntries = PINYIN_DICTIONARY;

  protected readonly parsedInput = computed(() =>
    parsePinyinInput(this.inputValue()),
  );
  protected readonly composition = computed(() =>
    composePinyinTokens(
      this.parsedInput().tokens,
      this.rememberedChoicesStore.choices(),
    ),
  );
  protected readonly ambiguousSegments = computed(() =>
    this.composition().segments.filter(
      (segment): segment is ComposedSegment => segment.isAmbiguous,
    ),
  );
  protected readonly rememberedChoiceCount = computed(
    () => Object.keys(this.rememberedChoicesStore.choices()).length,
  );
  protected readonly rememberedChoiceLabel = computed(() => {
    const count = this.rememberedChoiceCount();

    return `Clear ${count} remembered choice${count === 1 ? '' : 's'}`;
  });

  protected updateInput(value: string): void {
    this.inputValue.set(value);
  }

  protected rememberChoice(segmentKey: string, hanzi: string): void {
    this.rememberedChoicesStore.rememberChoice(segmentKey, hanzi);
  }

  protected clearRememberedChoices(): void {
    this.rememberedChoicesStore.clear();
  }
}
