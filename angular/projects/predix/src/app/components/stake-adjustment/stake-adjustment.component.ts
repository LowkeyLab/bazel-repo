import {
  ChangeDetectionStrategy,
  Component,
  input,
  output,
  computed,
} from '@angular/core';
import { CommonModule } from '@angular/common';
import { FormsModule } from '@angular/forms';

export interface StakeAdjustmentConfig {
  minStake: number;
  currentStake: number;
  isLoading: boolean;
}

@Component({
  selector: 'stake-adjustment',
  imports: [CommonModule, FormsModule],
  templateUrl: './stake-adjustment.component.html',
  styleUrls: ['./stake-adjustment.component.css'],
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class StakeAdjustmentComponent {
  readonly config = input.required<StakeAdjustmentConfig>();
  readonly stakeChanged = output<number>();
  readonly predictionSubmitted = output<void>();

  readonly adjustmentButtons = [
    { value: -100, label: '-100' },
    { value: -1000, label: '-1000' },
    { value: -10000, label: '-10000' },
    { value: 100, label: '+100' },
    { value: 1000, label: '+1000' },
    { value: 10000, label: '+10000' },
  ];

  readonly canDecreaseStake = computed(() => {
    const cfg = this.config();
    return cfg.currentStake > cfg.minStake && !cfg.isLoading;
  });

  onStakeChange(value: number): void {
    this.stakeChanged.emit(value);
  }

  onAdjustStake(delta: number): void {
    const cfg = this.config();
    const newValue = Math.max(cfg.minStake, cfg.currentStake + delta);
    this.stakeChanged.emit(newValue);
  }

  onSubmit(): void {
    this.predictionSubmitted.emit();
  }
}
