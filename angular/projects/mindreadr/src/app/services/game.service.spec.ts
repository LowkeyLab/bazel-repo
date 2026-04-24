import { TestBed } from '@angular/core/testing';
import {
  HttpTestingController,
  provideHttpClientTesting,
} from '@angular/common/http/testing';
import { firstValueFrom } from 'rxjs';
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

  it('should fetch summary from /games/summary', async () => {
    const mock: GameSummary = {
      waitingForPlayerGames: 2,
      inProgressGames: 3,
      completedGames: 4,
    };

    const summaryPromise = firstValueFrom(service.getSummary());

    const req = httpMock.expectOne(`${environment.API_BASE_URL}/games/summary`);
    expect(req.request.method).toBe('GET');
    req.flush(mock);
    await expect(summaryPromise).resolves.toEqual(mock);
  });

  it('getGamesByStatus should fetch /games?status=WAITING_FOR_PLAYERS', async () => {
    const mockGames: GameDto[] = [
      {
        id: 'G1',
        state: 'WAITING_FOR_PLAYERS',
        playerLimit: 2,
        roundLimit: 10,
        players: [],
        rounds: [],
      },
      {
        id: 'G2',
        state: 'WAITING_FOR_PLAYERS',
        playerLimit: 2,
        roundLimit: 10,
        players: [],
        rounds: [],
      },
    ];

    const gamesPromise = firstValueFrom(
      service.getGamesByStatus('WAITING_FOR_PLAYERS'),
    );

    const req = httpMock.expectOne(
      `${environment.API_BASE_URL}/games?status=WAITING_FOR_PLAYERS`,
    );
    expect(req.request.method).toBe('GET');
    req.flush(mockGames);
    await expect(gamesPromise).resolves.toEqual(mockGames);
  });

  it('getGamesByStatus should fetch /games?status=IN_PROGRESS', async () => {
    const mockGames: GameDto[] = [
      {
        id: 'IP1',
        state: 'IN_PROGRESS',
        playerLimit: 2,
        roundLimit: 10,
        players: [],
        rounds: [],
      },
      {
        id: 'IP2',
        state: 'IN_PROGRESS',
        playerLimit: 2,
        roundLimit: 10,
        players: [],
        rounds: [],
      },
      {
        id: 'IP3',
        state: 'IN_PROGRESS',
        playerLimit: 2,
        roundLimit: 10,
        players: [],
        rounds: [],
      },
    ];

    const gamesPromise = firstValueFrom(
      service.getGamesByStatus('IN_PROGRESS'),
    );

    const req = httpMock.expectOne(
      `${environment.API_BASE_URL}/games?status=IN_PROGRESS`,
    );
    expect(req.request.method).toBe('GET');
    req.flush(mockGames);
    await expect(gamesPromise).resolves.toEqual(mockGames);
  });

  it('getGamesByStatus should fetch /games?status=COMPLETED', async () => {
    const mockGames: GameDto[] = [
      {
        id: 'C1',
        state: 'COMPLETED',
        playerLimit: 2,
        roundLimit: 10,
        players: [],
        rounds: [],
      },
    ];

    const gamesPromise = firstValueFrom(service.getGamesByStatus('COMPLETED'));

    const req = httpMock.expectOne(
      `${environment.API_BASE_URL}/games?status=COMPLETED`,
    );
    expect(req.request.method).toBe('GET');
    req.flush(mockGames);
    await expect(gamesPromise).resolves.toEqual(mockGames);
  });
});
