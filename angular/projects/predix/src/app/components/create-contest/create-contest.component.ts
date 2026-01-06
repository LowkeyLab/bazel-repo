import {
  ChangeDetectionStrategy,
  Component,
  signal,
  inject,
  OnInit,
} from '@angular/core';
import { Router, ActivatedRoute } from '@angular/router';
import { FormsModule } from '@angular/forms';
import { ContestService } from '../../services/contest.service';
import { AuthService } from '../../services/auth.service';
import { BackButtonComponent } from '../back-button/back-button.component';

@Component({
  selector: 'create-contest',
  imports: [FormsModule, BackButtonComponent],
  templateUrl: './create-contest.component.html',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class CreateContestComponent implements OnInit {
  private readonly router = inject(Router);
  private readonly route = inject(ActivatedRoute);
  private readonly contestService = inject(ContestService);
  protected readonly auth = inject(AuthService);

  protected question = '';
  protected circleName = '';
  protected circleId: number | null = null;
  protected duration = '1d'; // Default to 1 day
  protected readonly minStakeOptions = [10, 100, 1000];
  protected selectedMinStake = this.minStakeOptions[0];
  protected readonly options = signal([{ value: '' }, { value: '' }]);
  protected readonly loading = signal(false);
  protected readonly error = signal('');

  ngOnInit(): void {
    const id = this.route.snapshot.paramMap.get('id');
    const name = this.route.snapshot.queryParamMap.get('circleName');

    if (id) {
      this.circleId = Number(id);
    }
    if (name) {
      this.circleName = name;
    }
  }

  protected addOption(): void {
    this.options.update((opts) => [...opts, { value: '' }]);
  }

  protected removeOption(index: number): void {
    this.options.update((opts) => opts.filter((_, i) => i !== index));
  }

  protected onSubmit(event: Event): void {
    event.preventDefault();

    // Validate
    if (!this.question.trim()) {
      this.error.set('Question is required');
      return;
    }

    if (!this.circleId) {
      this.error.set('Circle is required');
      return;
    }

    const optionTexts = this.options()
      .map((opt) => opt.value.trim())
      .filter((text) => text.length > 0);

    if (optionTexts.length < 2) {
      this.error.set('At least 2 options are required');
      return;
    }

    if (!this.duration) {
      this.error.set('Duration is required');
      return;
    }

    if (!this.minStakeOptions.includes(this.selectedMinStake)) {
      this.error.set('Select a minimum stake');
      return;
    }

    this.loading.set(true);
    this.error.set('');

    this.contestService
      .createContest({
        circle_id: this.circleId,
        question: this.question,
        options: optionTexts,
        min_stake: this.selectedMinStake,
        expiration_duration: this.duration,
      })
      .subscribe({
        next: (contest) => {
          this.loading.set(false);
          this.router.navigate([
            '/circles',
            contest.circle_id,
            'contests',
            contest.id,
          ]);
        },
        error: (err) => {
          this.loading.set(false);
          this.error.set(err.error?.error || 'Failed to create contest');
        },
      });
  }

  protected getBackLink(): string | string[] {
    if (this.circleId) {
      return ['/circles', `${this.circleId}`];
    }
    return '/contests';
  }

  protected navigateBack(): void {
    const link = this.getBackLink();
    this.router.navigate(Array.isArray(link) ? link : [link]);
  }
}
