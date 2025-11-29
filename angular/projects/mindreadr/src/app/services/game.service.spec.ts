import { TestBed } from '@angular/core/testing';
import { HttpClientTestingModule, HttpTestingController } from '@angular/common/http/testing';
import { GameService, GameSummary } from './game.service';
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
});
