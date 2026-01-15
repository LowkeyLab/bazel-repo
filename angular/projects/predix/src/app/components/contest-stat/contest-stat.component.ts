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
  styleUrls: ['./contest-stat.component.css'],
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: {
    class: 'stat transition-colors duration-300',
    '[class.flash-up]': 'flash() === "up"',
    '[class.flash-down]': 'flash() === "down"',
  },
})
export class ContestStatComponent {
  public readonly title = input.required<string>();
  public readonly value = input.required<number>();

  public readonly flash = signal<'up' | 'down' | null>(null);

  private previousValue: number | null = null;
  private timeoutId: any;

  constructor() {
    effect(() => {
      const current = this.value();

      if (this.previousValue !== null) {
        if (current > this.previousValue) {
          this.triggerFlash('up');
        } else if (current < this.previousValue) {
          this.triggerFlash('down');
        }
      }
      this.previousValue = current;
    });
  }

  private triggerFlash(direction: 'up' | 'down') {
    this.flash.set(direction);
    if (this.timeoutId) clearTimeout(this.timeoutId);
    this.timeoutId = setTimeout(() => this.flash.set(null), 1000);
  }
}
