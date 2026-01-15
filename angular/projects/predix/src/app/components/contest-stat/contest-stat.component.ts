import {
  ChangeDetectionStrategy,
  Component,
  effect,
  input,
  signal,
} from '@angular/core';

@Component({
  selector: 'contest-stat',
  standalone: true,
  templateUrl: './contest-stat.component.html',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ContestStatComponent {
  public readonly title = input.required<string>();
  public readonly value = input.required<number>();

  protected readonly flash = signal<'up' | 'down' | null>(null);

  private readonly previousValue = signal<number | null>(null);
  private timeoutId: ReturnType<typeof setTimeout> | null = null;

  private readonly FLASH_DURATION = 300;

  constructor() {
    effect((onCleanup) => {
      const current = this.value();
      const previous = this.previousValue();
      if (previous) {
        if (current > previous) {
          this.triggerFlash('up');
        } else if (current < previous) {
          this.triggerFlash('down');
        }
      }
      this.previousValue.set(current);
      onCleanup(() => {
        if (this.timeoutId) {
          clearTimeout(this.timeoutId);
          this.timeoutId = null;
        }
      });
    });
  }

  private triggerFlash(direction: 'up' | 'down') {
    this.flash.set(direction);
    if (this.timeoutId) clearTimeout(this.timeoutId);
    this.timeoutId = setTimeout(
      () => this.flash.set(null),
      this.FLASH_DURATION,
    );
  }
}
