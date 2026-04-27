import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  signal,
} from '@angular/core';

import type { HanziGuess } from './pinyinchch.service';
import { PinyinchchService } from './pinyinchch.service';

@Component({
  selector: 'app-root',
  templateUrl: './app.html',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class App {
  private readonly pinyinchch = inject(PinyinchchService);

  protected readonly input = signal('nihao');
  protected readonly guesses = signal<readonly HanziGuess[]>([]);
  protected readonly isLoading = signal(false);
  protected readonly errorMessage = signal<string | null>(null);
  protected readonly hasGuesses = computed(() => this.guesses().length > 0);

  protected updateInput(event: Event): void {
    const target = event.target;

    if (target instanceof HTMLTextAreaElement) {
      this.input.set(target.value);
    }
  }

  protected async guessHanzi(): Promise<void> {
    this.isLoading.set(true);
    this.errorMessage.set(null);

    try {
      const guesses = await this.pinyinchch.guess(this.input());
      this.guesses.set(guesses);
    } catch (error: unknown) {
      this.guesses.set([]);
      this.errorMessage.set(
        error instanceof Error ? error.message : 'Could not guess hanzi',
      );
    } finally {
      this.isLoading.set(false);
    }
  }
}
