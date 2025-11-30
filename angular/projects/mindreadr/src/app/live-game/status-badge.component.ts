import { Component, Input } from '@angular/core';
import { CommonModule } from '@angular/common';

/** Reusable status badge with optional ping animation using DaisyUI status classes. */
@Component({
  selector: 'mindreadr-status-badge',
  standalone: true,
  imports: [CommonModule],
  template: `
    <span
      class="inline-flex items-center gap-2"
      role="status"
      [attr.aria-live]="ariaLive"
      [attr.aria-label]="ariaLabel || label"
    >
      <span class="inline-grid *:[grid-area:1/1]" aria-hidden="true">
        @if (ping) {
          <span class="status" [ngClass]="statusClass + ' animate-ping'"></span>
        }
        <span class="status" [ngClass]="statusClass"></span>
      </span>
      <span>{{ label }}</span>
    </span>
  `,
})
export class StatusBadgeComponent {
  @Input() type: 'success' | 'warning' | 'info' | 'error' | 'neutral' = 'neutral';
  @Input() label = '';
  @Input() ping = false;
  /** Optional override for aria-label (defaults to label text) */
  @Input() ariaLabel: string | null = null;
  /** aria-live politeness; defaults to polite for dynamic status changes */
  @Input() ariaLive: 'off' | 'polite' | 'assertive' = 'polite';

  get statusClass(): string {
    return `status-${this.type}`;
  }
}
