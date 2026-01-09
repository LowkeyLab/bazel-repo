import { TestBed, fakeAsync, tick } from '@angular/core/testing';
import { ActivatedRoute, Router } from '@angular/router';
import { GameTimeoutComponent } from './game-timeout.component';
import { GameDto } from '../services/game.types';

describe('GameTimeoutComponent', () => {
  const TEST_ANIMATION_DELAY = 50; // Use faster delay for tests

  function createComponentWithNavState(state: any, id: string | null = 'g1') {
    const routerSpy = {
      currentNavigation: () => ({ extras: { state } }),
      navigate: jasmine.createSpy('navigate'),
    } as any as Router;
    TestBed.configureTestingModule({
      imports: [GameTimeoutComponent],
      providers: [
        { provide: Router, useValue: routerSpy },
        {
          provide: ActivatedRoute,
          useValue: { snapshot: { paramMap: { get: () => id } } },
        },
      ],
    });
    const fixture = TestBed.createComponent(GameTimeoutComponent);
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
        { number: 1, guesses: { Alice: 'Sunflower', Bob: 'Daisy' } },
        { number: 2, guesses: { Alice: 'Rose', Bob: 'Tulip' } },
      ],
      state: 'TERMINATED',
    };
    const { comp } = createComponentWithNavState({
      playerName: 'Alice',
      finalGame,
    });
    expect(comp.getPlayerName({})).toBe('Alice');
    expect(comp.game()).toEqual(finalGame);
  });

  it('shows inline error and redirects when finalGame is missing', (done) => {
    const { comp, routerSpy } = createComponentWithNavState({}, 'g1');
    expect(comp.error()).toContain('Game data unavailable');
    setTimeout(() => {
      expect(routerSpy.navigate).toHaveBeenCalledWith(['/games']);
      done();
    }, 1600);
  });

  it('orders rounds ascending by round number', fakeAsync(() => {
    const finalGame: GameDto = {
      id: 'g2',
      playerLimit: 2,
      roundLimit: 10,
      players: [{ name: 'Alice' }, { name: 'Bob' }],
      rounds: [
        { number: 3, guesses: { Bob: 'Tree', Alice: 'Forest' } },
        { number: 1, guesses: { Alice: 'Tree', Bob: 'Oak' } },
        { number: 2, guesses: { Alice: 'Pine', Bob: 'Birch' } },
      ],
      state: 'TERMINATED',
    };
    const { fixture } = createComponentWithNavState({
      playerName: 'Alice',
      finalGame,
    });

    // Wait for animation delays (3 rounds * delay)
    tick(3 * TEST_ANIMATION_DELAY);
    fixture.detectChanges();

    const el: HTMLElement = fixture.nativeElement as HTMLElement;
    const roundHeaders = Array.from(el.querySelectorAll('div.font-mono'));
    expect(roundHeaders.length).toBe(3);
    expect(roundHeaders[0].textContent?.trim()).toContain('Round #1');
    expect(roundHeaders[1].textContent?.trim()).toContain('Round #2');
    expect(roundHeaders[2].textContent?.trim()).toContain('Round #3');
  }));

  it('orders player names alphabetically in guesses', fakeAsync(() => {
    const finalGame: GameDto = {
      id: 'g3',
      playerLimit: 2,
      roundLimit: 10,
      players: [{ name: 'Bob' }, { name: 'Alice' }],
      rounds: [{ number: 1, guesses: { Bob: 'Moon', Alice: 'Sun' } }],
      state: 'TERMINATED',
    };
    const { fixture } = createComponentWithNavState({
      playerName: 'Bob',
      finalGame,
    });

    // Wait for animation delay (1 round * delay)
    tick(TEST_ANIMATION_DELAY);
    fixture.detectChanges();

    const el: HTMLElement = fixture.nativeElement as HTMLElement;
    const guessLabels = Array.from(
      el.querySelectorAll('div.font-semibold'),
    ).map((n) => (n.textContent || '').replace(':', '').trim());
    expect(guessLabels).toEqual(['Alice', 'Bob']);
  }));

  it('shows all rounds when game times out', fakeAsync(() => {
    const finalGame: GameDto = {
      id: 'g4',
      playerLimit: 2,
      roundLimit: 3,
      players: [{ name: 'Alice' }, { name: 'Bob' }],
      rounds: [
        { number: 1, guesses: { Alice: 'Cat', Bob: 'Dog' } },
        { number: 2, guesses: { Alice: 'Bird', Bob: 'Fish' } },
        { number: 3, guesses: { Alice: 'Mouse', Bob: 'Rat' } },
      ],
      state: 'TERMINATED',
    };
    const { fixture, comp } = createComponentWithNavState({
      playerName: 'Alice',
      finalGame,
    });

    // Wait for animation delays (3 rounds * delay)
    tick(3 * TEST_ANIMATION_DELAY);
    fixture.detectChanges();

    expect(comp.roundsCount()).toBe(3);

    const el: HTMLElement = fixture.nativeElement as HTMLElement;
    const roundHeaders = Array.from(el.querySelectorAll('div.font-mono'));
    expect(roundHeaders.length).toBe(3);
    expect(roundHeaders[0].textContent?.trim()).toContain('Round #1');
    expect(roundHeaders[2].textContent?.trim()).toContain('Round #3');
  }));
});
