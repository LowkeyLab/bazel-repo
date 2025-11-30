import { TestBed } from '@angular/core/testing';
import { LiveGameComponent } from './live-game.component';
import { ActivatedRoute, Router } from '@angular/router';
import { GameWsService, GameDto, RoundDto } from '../services/game-ws.service';

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
