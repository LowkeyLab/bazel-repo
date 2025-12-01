import { TestBed } from '@angular/core/testing';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { GameService, GameSummary } from './game.service';
import { GameDto } from '../services/game.types';
import { environment } from '../../environments/environment';
import { provideHttpClient } from '@angular/common/http';

describe('GameService', () => {
  let service: GameService;
  let httpMock: HttpTestingController;

  beforeEach(() => {
    TestBed.configureTestingModule({
      providers: [GameService, provideHttpClient(), provideHttpClientTesting()],
    });

    service = TestBed.inject(GameService);
    httpMock = TestBed.inject(HttpTestingController);
  });

  afterEach(() => {
    httpMock.verify();
  });

  it('should fetch summary from /games/summary', (done) => {
    const mock: GameSummary = {
      waitingForPlayerGames: 2,
      inProgressGames: 3,
      completedGames: 4,
    };

    service.getSummary().subscribe((res) => {
      expect(res).toEqual(mock);
      done();
    });

    const req = httpMock.expectOne(`${environment.API_BASE_URL}/games/summary`);
    expect(req.request.method).toBe('GET');
    req.flush(mock);
  });

  // Deprecated count helper tests removed. Components should consume getSummary() directly.

  it('getGamesByStatus should fetch /games?status=WAITING_FOR_PLAYERS', (done) => {
    const mockGames: GameDto[] = [
      { id: 'G1', state: 'WAITING_FOR_PLAYERS', playerLimit: 2, players: [], rounds: [] },
      { id: 'G2', state: 'WAITING_FOR_PLAYERS', playerLimit: 2, players: [], rounds: [] },
    ];

    service.getGamesByStatus('WAITING_FOR_PLAYERS').subscribe((games) => {
      expect(games).toEqual(mockGames);
      expect(games.length).toBe(2);
      done();
    });

    const req = httpMock.expectOne(`${environment.API_BASE_URL}/games?status=WAITING_FOR_PLAYERS`);
    expect(req.request.method).toBe('GET');
    req.flush(mockGames);
  });

  it('getGamesByStatus should fetch /games?status=IN_PROGRESS', (done) => {
    const mockGames: GameDto[] = [
      { id: 'IP1', state: 'IN_PROGRESS', playerLimit: 2, players: [], rounds: [] },
      { id: 'IP2', state: 'IN_PROGRESS', playerLimit: 2, players: [], rounds: [] },
      { id: 'IP3', state: 'IN_PROGRESS', playerLimit: 2, players: [], rounds: [] },
    ];

    service.getGamesByStatus('IN_PROGRESS').subscribe((games) => {
      expect(games).toEqual(mockGames);
      expect(games.every((g) => g.state === 'IN_PROGRESS')).toBeTrue();
      done();
    });

    const req = httpMock.expectOne(`${environment.API_BASE_URL}/games?status=IN_PROGRESS`);
    expect(req.request.method).toBe('GET');
    req.flush(mockGames);
  });

  it('getGamesByStatus should fetch /games?status=COMPLETED', (done) => {
    const mockGames: GameDto[] = [
      { id: 'C1', state: 'COMPLETED', playerLimit: 2, players: [], rounds: [] },
    ];

    service.getGamesByStatus('COMPLETED').subscribe((games) => {
      expect(games).toEqual(mockGames);
      expect(games.length).toBe(1);
      expect(games[0].state).toBe('COMPLETED');
      done();
    });

    const req = httpMock.expectOne(`${environment.API_BASE_URL}/games?status=COMPLETED`);
    expect(req.request.method).toBe('GET');
    req.flush(mockGames);
  });
});
