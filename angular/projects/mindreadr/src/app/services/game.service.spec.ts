import { TestBed } from '@angular/core/testing';
import { HttpClientTestingModule, HttpTestingController } from '@angular/common/http/testing';
import { GameService, GameSummary, Game } from './game.service';
import { environment } from '../../environments/environment';

describe('GameService', () => {
  let service: GameService;
  let httpMock: HttpTestingController;

  beforeEach(() => {
    TestBed.configureTestingModule({
      imports: [HttpClientTestingModule],
      providers: [GameService],
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

  it('getGamesCount should map to inProgressGames', (done) => {
    const mock: GameSummary = {
      waitingForPlayerGames: 0,
      inProgressGames: 7,
      completedGames: 1,
    };

    service.getGamesCount().subscribe((count) => {
      expect(count).toBe(7);
      done();
    });

    const req = httpMock.expectOne(`${environment.API_BASE_URL}/games/summary`);
    expect(req.request.method).toBe('GET');
    req.flush(mock);
  });

  it('getOpenGamesCount should map to waitingForPlayerGames', (done) => {
    const mock: GameSummary = {
      waitingForPlayerGames: 5,
      inProgressGames: 0,
      completedGames: 0,
    };

    service.getOpenGamesCount().subscribe((count) => {
      expect(count).toBe(5);
      done();
    });

    const req = httpMock.expectOne(`${environment.API_BASE_URL}/games/summary`);
    expect(req.request.method).toBe('GET');
    req.flush(mock);
  });

  it('getCompletedGamesCount should map to completedGames', (done) => {
    const mock: GameSummary = {
      waitingForPlayerGames: 0,
      inProgressGames: 0,
      completedGames: 9,
    };

    service.getCompletedGamesCount().subscribe((count) => {
      expect(count).toBe(9);
      done();
    });

    const req = httpMock.expectOne(`${environment.API_BASE_URL}/games/summary`);
    expect(req.request.method).toBe('GET');
    req.flush(mock);
  });

  it('getGamesByStatus should fetch /games?status=WAITING_FOR_PLAYERS', (done) => {
    const mockGames: Game[] = [
      { id: 'G1', state: 'WAITING_FOR_PLAYERS' },
      { id: 'G2', state: 'WAITING_FOR_PLAYERS' },
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
    const mockGames: Game[] = [
      { id: 'IP1', state: 'IN_PROGRESS' },
      { id: 'IP2', state: 'IN_PROGRESS' },
      { id: 'IP3', state: 'IN_PROGRESS' },
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
    const mockGames: Game[] = [{ id: 'C1', state: 'COMPLETED' }];

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
