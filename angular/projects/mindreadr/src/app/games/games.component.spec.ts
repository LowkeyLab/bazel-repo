import {
  ComponentFixture,
  TestBed,
  fakeAsync,
  tick,
  discardPeriodicTasks,
} from '@angular/core/testing';
import { GamesComponent } from './games.component';
import { GameService } from '../services/game.service';
import { GameDto } from '../services/game.types';
import { Component } from '@angular/core';
import { of, throwError } from 'rxjs';
import { Router } from '@angular/router';

// Mock GameService with spies
class MockGameService {
  createGame = jasmine
    .createSpy()
    .and.returnValue(of({ id: 'new-game' } as GameDto));
  getGamesByStatus = jasmine
    .createSpy()
    .and.returnValue(of([{ id: 'g1' } as GameDto]));
}

// Optional host for template mounting scenarios
@Component({
  template: '<mindreadr-games></mindreadr-games>',
  standalone: true,
  imports: [GamesComponent],
})
class TestHost {}

describe('GamesComponent', () => {
  let fixture: ComponentFixture<GamesComponent>;
  let component: GamesComponent;
  let gameService: MockGameService;

  beforeEach(async () => {
    await TestBed.configureTestingModule({
      imports: [GamesComponent],
      providers: [
        { provide: GameService, useClass: MockGameService },
        {
          provide: Router,
          useValue: { navigate: jasmine.createSpy('navigate') },
        },
      ],
    }).compileComponents();

    fixture = TestBed.createComponent(GamesComponent);
    component = fixture.componentInstance;
    gameService = TestBed.inject(GameService) as any;
  });

  it('should create', () => {
    expect(component).toBeTruthy();
  });

  it('fetches waiting games on init and sets state', () => {
    gameService.getGamesByStatus.and.returnValue(of([{ id: 'a' } as GameDto]));
    fixture.detectChanges();
    expect(gameService.getGamesByStatus).toHaveBeenCalledWith(
      'WAITING_FOR_PLAYERS',
    );
    expect(component.games().length).toBe(1);
    expect(component.loading()).toBeFalse();
    expect(component.error()).toBeNull();
  });

  it('handles error when fetching games', () => {
    gameService.getGamesByStatus.and.returnValue(
      throwError(() => new Error('boom')),
    );
    component.fetchWaitingGames();
    expect(component.error()).toBe('Failed to load games');
    expect(component.loading()).toBeFalse();
  });

  it('shows spinner while loading', () => {
    // Initialize template first, then toggle loading
    fixture.detectChanges();
    component.loading.set(true);
    fixture.detectChanges();
    const spinner = fixture.nativeElement.querySelector(
      '[aria-label="Loading open games"]',
    );
    expect(spinner).toBeTruthy();
  });

  it('renders list items when games exist', () => {
    // Prevent ngOnInit fetch from overwriting our state
    gameService.getGamesByStatus.and.returnValue(of([]));
    fixture.detectChanges();
    component.loading.set(false);
    component.games.set([{ id: '1' } as GameDto, { id: '2' } as GameDto]);
    fixture.detectChanges();
    const items = fixture.nativeElement.querySelectorAll('ul li');
    expect(items.length).toBe(2);
  });

  it('disables Create Game button when loading', () => {
    // Ensure DOM is rendered
    fixture.detectChanges();
    component.loading.set(true);
    fixture.detectChanges();
    const buttons: HTMLButtonElement[] = Array.from(
      fixture.nativeElement.querySelectorAll('button'),
    );
    const createBtn = buttons.find((b) =>
      b.textContent?.includes('Create Game'),
    ) as HTMLButtonElement;
    expect(createBtn).toBeTruthy();
    expect(createBtn.disabled).toBeTrue();
  });

  it('calls createGame and refreshes list on createNewGame()', () => {
    // Avoid ngOnInit interference: reset spies for clarity
    gameService.getGamesByStatus.calls.reset();
    gameService.createGame.calls.reset();

    gameService.createGame.and.returnValue(of({ id: 'created' } as GameDto));
    gameService.getGamesByStatus.and.returnValue(
      of([{ id: 'created' } as GameDto]),
    );

    component.createNewGame();

    expect(gameService.createGame).toHaveBeenCalled();
    expect(gameService.getGamesByStatus).toHaveBeenCalledWith(
      'WAITING_FOR_PLAYERS',
    );
    expect(component.loading()).toBeFalse();
    expect(component.games().length).toBe(1);
  });

  it('joinGame navigates to live route', () => {
    const router = TestBed.inject(Router) as any;
    component.joinGame('123');
    expect(router.navigate).toHaveBeenCalledWith(['/games', '123', 'live']);
  });

  it('starts polling for games every 1 second after init', fakeAsync(() => {
    gameService.getGamesByStatus.calls.reset();
    gameService.getGamesByStatus.and.returnValue(
      of([{ id: 'game1' } as GameDto]),
    );

    fixture.detectChanges(); // triggers ngOnInit

    // Initial fetch happens immediately
    expect(gameService.getGamesByStatus).toHaveBeenCalledTimes(1);

    // After 1 second, polling should trigger another fetch
    tick(1000);
    expect(gameService.getGamesByStatus).toHaveBeenCalledTimes(2);

    // After another second, another fetch
    tick(1000);
    expect(gameService.getGamesByStatus).toHaveBeenCalledTimes(3);

    discardPeriodicTasks();
  }));

  it('stops polling when component is destroyed', fakeAsync(() => {
    gameService.getGamesByStatus.calls.reset();
    gameService.getGamesByStatus.and.returnValue(
      of([{ id: 'game1' } as GameDto]),
    );

    fixture.detectChanges(); // triggers ngOnInit

    // Initial fetch + first poll
    tick(1000);
    expect(gameService.getGamesByStatus).toHaveBeenCalledTimes(2);

    // Destroy the component
    component.ngOnDestroy();

    // After another second, no more fetches should occur
    tick(1000);
    expect(gameService.getGamesByStatus).toHaveBeenCalledTimes(2);

    discardPeriodicTasks();
  }));

  it('updates games list when polling returns new data', fakeAsync(() => {
    gameService.getGamesByStatus.and.returnValue(
      of([{ id: 'initial' } as GameDto]),
    );
    fixture.detectChanges(); // triggers ngOnInit
    expect(component.games().length).toBe(1);
    expect(component.games()[0].id).toBe('initial');

    // Simulate new game appearing
    gameService.getGamesByStatus.and.returnValue(
      of([{ id: 'initial' } as GameDto, { id: 'new' } as GameDto]),
    );
    tick(1000);

    expect(component.games().length).toBe(2);
    expect(component.games()[1].id).toBe('new');

    discardPeriodicTasks();
  }));

  it('silently handles polling errors without affecting displayed games', fakeAsync(() => {
    gameService.getGamesByStatus.and.returnValue(
      of([{ id: 'game1' } as GameDto]),
    );
    fixture.detectChanges();
    expect(component.games().length).toBe(1);
    expect(component.error()).toBeNull();

    // Make the next poll fail
    gameService.getGamesByStatus.and.returnValue(
      throwError(() => new Error('network error')),
    );
    tick(1000);

    // Games should still be displayed (no error shown for polling failures)
    expect(component.games().length).toBe(1);
    expect(component.error()).toBeNull();

    discardPeriodicTasks();
  }));
});
