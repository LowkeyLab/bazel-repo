import { TestBed } from '@angular/core/testing';
import { HttpClientTestingModule, HttpTestingController } from '@angular/common/http/testing';
import { GameService } from './game.service';

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

  it('should be created', () => {
    expect(service).toBeTruthy();
  });

  it('should POST to /games and return a game', () => {
    const mockResponse: { id: string } = { id: 'game-123' };

    service.createGame().subscribe((game) => {
      expect(game).toBeTruthy();
      expect(game.id).toBe('game-123');
    });

    const req = httpMock.expectOne((r) => r.method === 'POST' && r.url.includes('/games'));
    expect(req.request.body).toEqual({});
    req.flush(mockResponse);
  });

  it('should surface errors from backend', () => {
    service.createGame().subscribe({
      next: () => fail('expected an error'),
      error: (err) => {
        expect(err.status).toBe(500);
      },
    });

    const req = httpMock.expectOne((r) => r.method === 'POST' && r.url.includes('/games'));
    req.flush({ message: 'server error' }, { status: 500, statusText: 'Server Error' });
  });
  it('should GET games and return number in progress', () => {
    const mockResponse = [
      { id: '1', state: 'WAITING_FOR_PLAYERS' },
      { id: '2', state: 'IN_PROGRESS' },
      { id: '3', state: 'IN_PROGRESS' },
      { id: '4', state: 'COMPLETED' },
    ];
    service.getGamesCount().subscribe((count) => {
      expect(count).toBe(2);
    });
    const req = httpMock.expectOne((r) => r.method === 'GET' && r.url.includes('/games'));
    req.flush(mockResponse);
  });

  it('should surface errors from getGamesCount', () => {
    service.getGamesCount().subscribe({
      next: () => fail('expected an error'),
      error: (err) => {
        expect(err.status).toBe(404);
      },
    });
    const req = httpMock.expectOne((r) => r.method === 'GET' && r.url.includes('/games'));
    req.flush({ message: 'not found' }, { status: 404, statusText: 'Not Found' });
  });

  it('should GET open games and return number waiting for players', () => {
    const mockResponse = [
      { id: '1', state: 'WAITING_FOR_PLAYERS' },
      { id: '2', state: 'IN_PROGRESS' },
      { id: '3', state: 'WAITING_FOR_PLAYERS' },
      { id: '4', state: 'COMPLETED' },
    ];
    service.getOpenGamesCount().subscribe((count) => {
      expect(count).toBe(2);
    });
    const req = httpMock.expectOne((r) => r.method === 'GET' && r.url.includes('/games'));
    req.flush(mockResponse);
  });
});
