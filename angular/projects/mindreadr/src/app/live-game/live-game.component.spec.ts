import { TestBed } from '@angular/core/testing';
import { vi } from 'vitest';
import { LiveGameComponent } from './live-game.component';
import { ActivatedRoute, Router } from '@angular/router';
import { GameWsService } from '../services/game-ws.service';
import { GameDto, RoundDto } from '../services/game.types';
import { Subject } from 'rxjs';

describe('LiveGameComponent logic', () => {
  let component: LiveGameComponent;

  beforeEach(() => {
    TestBed.configureTestingModule({
      imports: [LiveGameComponent],
      providers: [
        {
          provide: ActivatedRoute,
          useValue: { snapshot: { paramMap: { get: () => null } } },
        },
        { provide: GameWsService, useValue: {} },
        {
          provide: Router,
          useValue: { navigate: vi.fn() },
        },
      ],
    });
    const fixture = TestBed.createComponent(LiveGameComponent);
    component = fixture.componentInstance;
  });

  function makePlayer(name: string, id?: string) {
    return id ? { name, id } : { name };
  }

  it('hides guesses and waits when only one player has guessed', () => {
    const p1 = makePlayer('Alice', 'a1');
    const p2 = makePlayer('Bob', 'b2');
    const round: RoundDto = {
      number: 1,
      guesses: { Alice: 'Sunflower' },
    };
    const game: GameDto = {
      id: 'g1',
      playerLimit: 2,
      roundLimit: 10,
      players: [p1, p2],
      rounds: [round],
      state: 'IN_PROGRESS',
    };
    component.game.set(game);
    component.currentPlayer.set(p1);

    expect(component.shouldHideGuesses(round, game)).toBe(true);
    expect(component.waitingForOtherPlayer()).toBe(true);
  });

  it('reveals guesses and not waiting when all players have guessed', () => {
    const p1 = makePlayer('Alice', 'a1');
    const p2 = makePlayer('Bob', 'b2');
    const round: RoundDto = {
      number: 1,
      guesses: { Alice: 'Sunflower', Bob: 'Moon' },
    };
    const game: GameDto = {
      id: 'g1',
      playerLimit: 2,
      roundLimit: 10,
      players: [p1, p2],
      rounds: [round],
      state: 'IN_PROGRESS',
    };
    component.game.set(game);
    component.currentPlayer.set(p1);

    expect(component.shouldHideGuesses(round, game)).toBe(false);
    expect(component.waitingForOtherPlayer()).toBe(false);
  });

  it('detects player guess via id fallback when name key missing', () => {
    const p1 = makePlayer('Alice', 'a1');
    const p2 = makePlayer('Bob', 'b2');
    const round: RoundDto = {
      number: 1,
      guesses: { a1: 'Cloud', b2: 'Rain' },
    };
    const game: GameDto = {
      id: 'g1',
      playerLimit: 2,
      roundLimit: 10,
      players: [p1, p2],
      rounds: [round],
      state: 'IN_PROGRESS',
    };
    component.game.set(game);
    component.currentPlayer.set(p1);

    // All guessed -> not hidden
    expect(component.shouldHideGuesses(round, game)).toBe(false);
    expect(component.waitingForOtherPlayer()).toBe(false);
  });

  it('returns empty array for reversedRounds when game is null', () => {
    component.game.set(null);
    expect(component.reversedRounds()).toEqual([]);
  });

  it('returns reversed rounds in correct order (most recent first)', () => {
    const rounds: RoundDto[] = [
      { number: 1, guesses: { Alice: 'Cat' } },
      { number: 2, guesses: { Alice: 'Dog' } },
      { number: 3, guesses: { Alice: 'Bird' } },
    ];
    const game: GameDto = {
      id: 'g1',
      playerLimit: 2,
      roundLimit: 10,
      players: [{ name: 'Alice' }],
      rounds: rounds,
      state: 'IN_PROGRESS',
    };
    component.game.set(game);

    const reversed = component.reversedRounds();
    expect(reversed.length).toBe(3);
    expect(reversed[0].number).toBe(3);
    expect(reversed[1].number).toBe(2);
    expect(reversed[2].number).toBe(1);
  });

  it('updates reversedRounds when game state changes', () => {
    const game1: GameDto = {
      id: 'g1',
      playerLimit: 2,
      roundLimit: 10,
      players: [{ name: 'Alice' }],
      rounds: [{ number: 1, guesses: { Alice: 'Cat' } }],
      state: 'IN_PROGRESS',
    };
    component.game.set(game1);

    expect(component.reversedRounds().length).toBe(1);
    expect(component.reversedRounds()[0].number).toBe(1);

    // Update game with more rounds
    const game2: GameDto = {
      ...game1,
      rounds: [
        { number: 1, guesses: { Alice: 'Cat' } },
        { number: 2, guesses: { Alice: 'Dog' } },
      ],
    };
    component.game.set(game2);

    expect(component.reversedRounds().length).toBe(2);
    expect(component.reversedRounds()[0].number).toBe(2);
    expect(component.reversedRounds()[1].number).toBe(1);
  });

  it('does not mutate original rounds array', () => {
    const rounds: RoundDto[] = [
      { number: 1, guesses: { Alice: 'Cat' } },
      { number: 2, guesses: { Alice: 'Dog' } },
    ];
    const game: GameDto = {
      id: 'g1',
      playerLimit: 2,
      roundLimit: 10,
      players: [{ name: 'Alice' }],
      rounds: rounds,
      state: 'IN_PROGRESS',
    };
    component.game.set(game);

    const reversed = component.reversedRounds();

    // Original rounds should remain unchanged
    expect(game.rounds[0].number).toBe(1);
    expect(game.rounds[1].number).toBe(2);
    // Reversed should be opposite
    expect(reversed[0].number).toBe(2);
    expect(reversed[1].number).toBe(1);
  });
});

describe('LiveGameComponent navigation', () => {
  let component: LiveGameComponent;
  let router: Router;
  let gameState$: Subject<GameDto>;
  let errors$: Subject<string>;
  let terminated$: Subject<string>;
  let playerJoined$: Subject<any>;
  let playerLeft$: Subject<any>;

  beforeEach(() => {
    gameState$ = new Subject<GameDto>();
    errors$ = new Subject<string>();
    terminated$ = new Subject<string>();
    playerJoined$ = new Subject<any>();
    playerLeft$ = new Subject<any>();
    const mockConn = {
      gameState$,
      errors$,
      terminated$,
      playerJoined$,
      playerLeft$,
      submitGuess: vi.fn(),
      close: vi.fn(),
    };

    TestBed.configureTestingModule({
      imports: [LiveGameComponent],
      providers: [
        {
          provide: ActivatedRoute,
          useValue: { snapshot: { paramMap: { get: () => 'g1' } } },
        },
        { provide: GameWsService, useValue: { connect: () => mockConn } },
        {
          provide: Router,
          useValue: { navigate: vi.fn() },
        },
      ],
    });

    const fixture = TestBed.createComponent(LiveGameComponent);
    component = fixture.componentInstance;
    router = TestBed.inject(Router);
    fixture.detectChanges();
  });

  it('navigates to summary when game state becomes COMPLETED', () => {
    const game: GameDto = {
      id: 'g1',
      playerLimit: 2,
      roundLimit: 10,
      players: [],
      rounds: [],
      state: 'COMPLETED',
    };
    gameState$.next(game);
    expect(router.navigate).toHaveBeenCalledWith(['/games/g1/summary'], {
      state: expect.objectContaining({
        playerName: 'Player',
        finalGame: game,
      }),
    });
  });

  it('navigates to summary when terminated with COMPLETED', () => {
    terminated$.next('COMPLETED');
    expect(router.navigate).toHaveBeenCalledWith(['/games/g1/summary'], {
      state: expect.objectContaining({ playerName: 'Player' }),
    });
  });

  it('navigates to timeout when game state becomes TERMINATED and reached round limit', () => {
    const game: GameDto = {
      id: 'g1',
      playerLimit: 2,
      roundLimit: 2,
      players: [{ name: 'Alice' }, { name: 'Bob' }],
      rounds: [
        { number: 1, guesses: { Alice: 'Cat', Bob: 'Dog' } },
        { number: 2, guesses: { Alice: 'Bird', Bob: 'Fish' } },
      ],
      state: 'TERMINATED',
    };
    gameState$.next(game);
    expect(router.navigate).toHaveBeenCalledWith(['/games/g1/timeout'], {
      state: expect.objectContaining({
        playerName: 'Player',
        finalGame: game,
      }),
    });
  });

  it('does not navigate to timeout when TERMINATED but round limit not reached', () => {
    const game: GameDto = {
      id: 'g1',
      playerLimit: 2,
      roundLimit: 5,
      players: [{ name: 'Alice' }],
      rounds: [{ number: 1, guesses: { Alice: 'Cat' } }],
      state: 'TERMINATED',
    };
    gameState$.next(game);
    expect(router.navigate).not.toHaveBeenCalled();
  });

  it('navigates to timeout when terminated with TERMINATED and round limit reached', () => {
    const game: GameDto = {
      id: 'g1',
      playerLimit: 2,
      roundLimit: 3,
      players: [{ name: 'Alice' }, { name: 'Bob' }],
      rounds: [
        { number: 1, guesses: { Alice: 'Cat', Bob: 'Dog' } },
        { number: 2, guesses: { Alice: 'Bird', Bob: 'Fish' } },
        { number: 3, guesses: { Alice: 'Tree', Bob: 'Plant' } },
      ],
      state: 'TERMINATED',
    };
    component.game.set(game);
    terminated$.next('TERMINATED');
    expect(router.navigate).toHaveBeenCalledWith(['/games/g1/timeout'], {
      state: expect.objectContaining({ playerName: 'Player' }),
    });
  });

  it('does not navigate to timeout when terminated with TERMINATED but round limit not reached', () => {
    const game: GameDto = {
      id: 'g1',
      playerLimit: 2,
      roundLimit: 5,
      players: [{ name: 'Alice' }],
      rounds: [{ number: 1, guesses: { Alice: 'Cat' } }],
      state: 'TERMINATED',
    };
    component.game.set(game);
    terminated$.next('TERMINATED');
    expect(router.navigate).not.toHaveBeenCalled();
  });

  it('calculates rounds remaining correctly', () => {
    const game: GameDto = {
      id: 'g1',
      playerLimit: 2,
      roundLimit: 10,
      players: [],
      rounds: [
        { number: 1, guesses: {} },
        { number: 2, guesses: {} },
        { number: 3, guesses: {} },
      ],
      state: 'IN_PROGRESS',
    };
    component.game.set(game);
    expect(component.getRoundsRemaining(game)).toBe(7);
  });

  it('returns correct color for high rounds remaining', () => {
    const game: GameDto = {
      id: 'g1',
      playerLimit: 2,
      roundLimit: 10,
      players: [],
      rounds: [{ number: 1, guesses: {} }],
      state: 'IN_PROGRESS',
    };
    expect(component.getRoundsRemainingColor(game)).toBe('text-success');
  });

  it('returns correct color for medium rounds remaining', () => {
    const game: GameDto = {
      id: 'g1',
      playerLimit: 2,
      roundLimit: 10,
      players: [],
      rounds: [
        { number: 1, guesses: {} },
        { number: 2, guesses: {} },
        { number: 3, guesses: {} },
        { number: 4, guesses: {} },
        { number: 5, guesses: {} },
      ],
      state: 'IN_PROGRESS',
    };
    expect(component.getRoundsRemainingColor(game)).toBe('text-warning');
  });

  it('returns correct color for low rounds remaining', () => {
    const game: GameDto = {
      id: 'g1',
      playerLimit: 2,
      roundLimit: 10,
      players: [],
      rounds: [
        { number: 1, guesses: {} },
        { number: 2, guesses: {} },
        { number: 3, guesses: {} },
        { number: 4, guesses: {} },
        { number: 5, guesses: {} },
        { number: 6, guesses: {} },
        { number: 7, guesses: {} },
        { number: 8, guesses: {} },
      ],
      state: 'IN_PROGRESS',
    };
    expect(component.getRoundsRemainingColor(game)).toBe('text-error');
  });

  it('handles zero rounds remaining', () => {
    const game: GameDto = {
      id: 'g1',
      playerLimit: 2,
      roundLimit: 5,
      players: [],
      rounds: [
        { number: 1, guesses: {} },
        { number: 2, guesses: {} },
        { number: 3, guesses: {} },
        { number: 4, guesses: {} },
        { number: 5, guesses: {} },
      ],
      state: 'IN_PROGRESS',
    };
    expect(component.getRoundsRemaining(game)).toBe(0);
  });

  it('handles game with no rounds yet', () => {
    const game: GameDto = {
      id: 'g1',
      playerLimit: 2,
      roundLimit: 10,
      players: [],
      rounds: [],
      state: 'IN_PROGRESS',
    };
    expect(component.getRoundsRemaining(game)).toBe(10);
  });
});
