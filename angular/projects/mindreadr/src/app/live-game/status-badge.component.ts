import { Component, Input } from '@angular/core';
import { CommonModule } from '@angular/common';

/** Reusable status badge with optional ping animation using DaisyUI status classes. */
@Component({
  selector: 'mindreadr-status-badge',
  standalone: true,
  imports: [CommonModule],
  template: `
    <span class="inline-flex items-center gap-2">
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

  get statusClass(): string {
    return `status-${this.type}`;
  }
}
