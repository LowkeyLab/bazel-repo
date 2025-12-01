import { TestBed } from '@angular/core/testing';
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
        { provide: ActivatedRoute, useValue: { snapshot: { paramMap: { get: () => null } } } },
        { provide: GameWsService, useValue: {} },
        { provide: Router, useValue: { navigate: jasmine.createSpy('navigate') } },
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

    expect(component.shouldHideGuesses(round, game)).toBeTrue();
    expect(component.waitingForOtherPlayer()).toBeTrue();
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

    expect(component.shouldHideGuesses(round, game)).toBeFalse();
    expect(component.waitingForOtherPlayer()).toBeFalse();
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
    expect(component.shouldHideGuesses(round, game)).toBeFalse();
    expect(component.waitingForOtherPlayer()).toBeFalse();
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
      submitGuess: jasmine.createSpy('submitGuess'),
      close: jasmine.createSpy('close'),
    };

    TestBed.configureTestingModule({
      imports: [LiveGameComponent],
      providers: [
        { provide: ActivatedRoute, useValue: { snapshot: { paramMap: { get: () => 'g1' } } } },
        { provide: GameWsService, useValue: { connect: () => mockConn } },
        { provide: Router, useValue: { navigate: jasmine.createSpy('navigate') } },
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
      state: jasmine.objectContaining({ playerName: 'Player', finalGame: game }),
    });
  });

  it('navigates to summary when terminated with COMPLETED', () => {
    terminated$.next('COMPLETED');
    expect(router.navigate).toHaveBeenCalledWith(['/games/g1/summary'], {
      state: jasmine.objectContaining({ playerName: 'Player' }),
    });
  });

  it('navigates to timeout when game state becomes TERMINATED', () => {
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
      state: jasmine.objectContaining({ playerName: 'Player', finalGame: game }),
    });
  });

  it('navigates to timeout when terminated with TERMINATED', () => {
    terminated$.next('TERMINATED');
    expect(router.navigate).toHaveBeenCalledWith(['/games/g1/timeout'], {
      state: jasmine.objectContaining({ playerName: 'Player' }),
    });
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
