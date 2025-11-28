import { TestBed } from '@angular/core/testing';
import { HttpClientTestingModule, HttpTestingController } from '@angular/common/http/testing';
import { GameService, Game } from './game.service';

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
    const mockResponse: Game = { id: 'game-123' };

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
});
