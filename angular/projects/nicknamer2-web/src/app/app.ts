import { ChangeDetectionStrategy, Component, signal } from '@angular/core';

@Component({
  selector: 'app-root',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: '<h1>{{ title() }}</h1>',
})
export class App {
  protected readonly title = signal('Nicknamer2 Web');
}
