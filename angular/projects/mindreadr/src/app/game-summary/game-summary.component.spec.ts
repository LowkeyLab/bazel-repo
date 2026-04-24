import { TestBed } from '@angular/core/testing';
import { vi } from 'vitest';
import { ActivatedRoute, Router } from '@angular/router';

vi.mock('canvas-confetti', () => ({
  default: vi.fn(),
}));
import { GameSummaryComponent } from './game-summary.component';
import { GameDto } from '../services/game.types';

describe('GameSummaryComponent', () => {
  const TEST_ANIMATION_DELAY = 50; // Use faster delay for tests

  afterEach(() => {
    vi.useRealTimers();
  });

  function createComponentWithNavState(state: any, id: string | null = 'g1') {
    const routerSpy = {
      currentNavigation: () => ({ extras: { state } }),
      navigate: vi.fn(),
    } as any as Router;
    TestBed.configureTestingModule({
      imports: [GameSummaryComponent],
      providers: [
        { provide: Router, useValue: routerSpy },
        {
          provide: ActivatedRoute,
          useValue: { snapshot: { paramMap: { get: () => id } } },
        },
      ],
    });
    const fixture = TestBed.createComponent(GameSummaryComponent);
    const comp = fixture.componentInstance;
    // Set test animation delay
    fixture.componentRef.setInput('animationDelayMs', TEST_ANIMATION_DELAY);
    fixture.detectChanges();
    return { fixture, comp, routerSpy };
  }

  it('renders with player name and final game from navigation state', () => {
    const finalGame: GameDto = {
      id: 'g1',
      playerLimit: 2,
      roundLimit: 10,
      players: [{ name: 'Alice' }, { name: 'Bob' }],
      rounds: [
        { number: 1, guesses: { Alice: 'Sunflower', Bob: 'Sunflower' } },
      ],
      state: 'COMPLETED',
    };
    const { comp } = createComponentWithNavState({
      playerName: 'Alice',
      finalGame,
    });
    expect(comp.getPlayerName({})).toBe('Alice');
    expect(comp.game()).toEqual(finalGame);
    expect(comp.guessedWord()).toBe('Sunflower');
  });

  it('shows inline error and redirects when finalGame is missing', async () => {
    vi.useFakeTimers();
    const { comp, routerSpy } = createComponentWithNavState({}, 'g1');
    expect(comp.error()).toContain('Summary unavailable');
    await vi.advanceTimersByTimeAsync(1600);
    expect(routerSpy.navigate).toHaveBeenCalledWith(['/games']);
  });

  it('orders rounds ascending by round number', async () => {
    vi.useFakeTimers();
    const finalGame: GameDto = {
      id: 'g2',
      playerLimit: 2,
      roundLimit: 10,
      players: [{ name: 'Alice' }, { name: 'Bob' }],
      rounds: [
        { number: 2, guesses: { Bob: 'Tree', Alice: 'Tree' } },
        { number: 1, guesses: { Alice: 'Tree', Bob: 'Tree' } },
      ],
      state: 'COMPLETED',
    };
    const { fixture } = createComponentWithNavState({
      playerName: 'Alice',
      finalGame,
    });

    await vi.advanceTimersByTimeAsync(2 * TEST_ANIMATION_DELAY);
    fixture.detectChanges();

    const el: HTMLElement = fixture.nativeElement as HTMLElement;
    const roundHeaders = Array.from(el.querySelectorAll('div.font-mono'));
    expect(roundHeaders.length).toBe(2);
    expect(roundHeaders[0].textContent?.trim()).toContain('Round #1');
    expect(roundHeaders[1].textContent?.trim()).toContain('Round #2');
  });

  it('orders player names alphabetically in guesses', async () => {
    vi.useFakeTimers();
    const finalGame: GameDto = {
      id: 'g3',
      playerLimit: 2,
      roundLimit: 10,
      players: [{ name: 'Bob' }, { name: 'Alice' }],
      rounds: [{ number: 1, guesses: { Bob: 'Moon', Alice: 'Moon' } }],
      state: 'COMPLETED',
    };
    const { fixture } = createComponentWithNavState({
      playerName: 'Bob',
      finalGame,
    });

    await vi.advanceTimersByTimeAsync(TEST_ANIMATION_DELAY);
    fixture.detectChanges();

    const el: HTMLElement = fixture.nativeElement as HTMLElement;
    const guessLabels = Array.from(
      el.querySelectorAll('div.font-semibold'),
    ).map((n) => (n.textContent || '').replace(':', '').trim());
    expect(guessLabels).toEqual(['Alice', 'Bob']);
  });
});
