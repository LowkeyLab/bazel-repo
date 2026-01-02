import { Component, input, ChangeDetectionStrategy } from '@angular/core';
import { Router } from '@angular/router';
import { Location } from '@angular/common';
import { inject } from '@angular/core';

@Component({
  selector: 'app-back-button',
  template: `
    <button class="btn btn-ghost" (click)="navigateBack()">
      ← {{ label() }}
    </button>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class BackButtonComponent {
  private readonly router = inject(Router);
  private readonly location = inject(Location);

  label = input('Back');
  link = input<string | string[] | null>(null);

  navigateBack(): void {
    const navigateTo = this.link();

    if (navigateTo) {
      this.router.navigate(
        Array.isArray(navigateTo) ? navigateTo : [navigateTo],
      );
    } else {
      this.location.back();
    }
  }
}
