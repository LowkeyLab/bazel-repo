import { ComponentFixture, TestBed } from '@angular/core/testing';
import { GamesCountComponent } from './games-count.component';
import { GameService } from '../services/game.service';
import { of, throwError } from 'rxjs';
import { Component } from '@angular/core';

// Mock GameService
class MockGameService {
  getSummary = jasmine
    .createSpy()
    .and.returnValue(
      of({ inProgressGames: 5, waitingForPlayerGames: 3, completedGames: 2 }),
    );
}

// Host component for standalone test
@Component({
  template: '<mindreadr-games-count></mindreadr-games-count>',
  standalone: true,
  imports: [GamesCountComponent],
})
class TestHost {}

describe('GamesCountComponent', () => {
  let fixture: ComponentFixture<GamesCountComponent>;
  let component: GamesCountComponent;
  let gameService: MockGameService;

  beforeEach(async () => {
    await TestBed.configureTestingModule({
      imports: [GamesCountComponent],
      providers: [{ provide: GameService, useClass: MockGameService }],
    }).compileComponents();
    fixture = TestBed.createComponent(GamesCountComponent);
    component = fixture.componentInstance;
    gameService = TestBed.inject(GameService) as any;
  });

  it('should create', () => {
    expect(component).toBeTruthy();
  });

  it('should call all services and set counts', () => {
    component.fetchCounts();
    expect(gameService.getSummary).toHaveBeenCalled();
    expect(component.count()).toBe(5);
    expect(component.openCount()).toBe(3);
    expect(component.completedCount()).toBe(2);
    expect(component.loading()).toBe(false);
    expect(component.error()).toBeNull();
  });

  it('should handle errors from either service', () => {
    gameService.getSummary.and.returnValue(
      throwError(() => ({ message: 'fail' })),
    );
    component.fetchCounts();
    expect(component.count()).toBeNull();
    expect(component.openCount()).toBeNull();
    expect(component.completedCount()).toBeNull();
    expect(component.loading()).toBe(false);
    expect(component.error()).toBe('fail');
  });
});
