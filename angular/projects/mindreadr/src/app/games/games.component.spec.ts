import { ComponentFixture, TestBed } from '@angular/core/testing';
import { GamesComponent } from './games.component';
import { GameService, Game } from '../services/game.service';
import { Component } from '@angular/core';
import { of, Subject, throwError } from 'rxjs';
import { Router } from '@angular/router';

// Mock GameService with spies
class MockGameService {
  createGame = jasmine.createSpy().and.returnValue(of({ id: 'new-game' } as Game));
  getGamesByStatus = jasmine.createSpy().and.returnValue(of([{ id: 'g1' } as Game]));
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
        { provide: Router, useValue: { navigate: jasmine.createSpy('navigate') } },
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
    gameService.getGamesByStatus.and.returnValue(of([{ id: 'a' } as Game]));
    fixture.detectChanges();
    expect(gameService.getGamesByStatus).toHaveBeenCalledWith('WAITING_FOR_PLAYERS');
    expect(component.games().length).toBe(1);
    expect(component.loading()).toBeFalse();
    expect(component.error()).toBeNull();
  });

  it('handles error when fetching games', () => {
    gameService.getGamesByStatus.and.returnValue(throwError(() => new Error('boom')));
    component.fetchWaitingGames();
    expect(component.error()).toBe('Failed to load games');
    expect(component.loading()).toBeFalse();
  });

  it('shows spinner while loading', () => {
    // Initialize template first, then toggle loading
    fixture.detectChanges();
    component.loading.set(true);
    fixture.detectChanges();
    const spinner = fixture.nativeElement.querySelector('[aria-label="Loading open games"]');
    expect(spinner).toBeTruthy();
  });

  it('renders list items when games exist', () => {
    // Prevent ngOnInit fetch from overwriting our state
    gameService.getGamesByStatus.and.returnValue(of([]));
    fixture.detectChanges();
    component.loading.set(false);
    component.games.set([{ id: '1' } as Game, { id: '2' } as Game]);
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

    gameService.createGame.and.returnValue(of({ id: 'created' } as Game));
    gameService.getGamesByStatus.and.returnValue(of([{ id: 'created' } as Game]));

    component.createNewGame();

    expect(gameService.createGame).toHaveBeenCalled();
    expect(gameService.getGamesByStatus).toHaveBeenCalledWith('WAITING_FOR_PLAYERS');
    expect(component.loading()).toBeFalse();
    expect(component.games().length).toBe(1);
  });

  it('joinGame navigates to live route', () => {
    const router = TestBed.inject(Router) as any;
    component.joinGame('123');
    expect(router.navigate).toHaveBeenCalledWith(['/games', '123', 'live']);
  });
});
