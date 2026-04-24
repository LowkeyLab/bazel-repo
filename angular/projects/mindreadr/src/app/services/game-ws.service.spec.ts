import { GameWsService, ServerMessage } from './game-ws.service';
import { vi } from 'vitest';
import { Subject, firstValueFrom, take } from 'rxjs';

// Allow mutating API_BASE_URL during tests
import { environment } from '../../environments/environment';

describe('GameWsService', () => {
  let service: GameWsService;

  // Fake socket harness
  type FakeSocket<T> = {
    incoming$: Subject<T>;
    sent: any[];
    completed: boolean;
    asObservable: () => any;
    next: (v: any) => void;
    complete: () => void;
  };

  let fake: FakeSocket<ServerMessage>;
  let capturedUrl: string | undefined;

  beforeEach(() => {
    service = new GameWsService();

    fake = {
      incoming$: new Subject<ServerMessage>(),
      sent: [],
      completed: false,
      asObservable() {
        return this.incoming$.asObservable();
      },
      next(v: any) {
        this.sent.push(v);
      },
      complete() {
        this.completed = true;
        this.incoming$.complete();
      },
    };

    capturedUrl = undefined;
    vi.spyOn(service as never, 'createSocket' as never).mockImplementation(
      (config: { url?: string }) => {
        capturedUrl = config?.url;
        return fake as never;
      },
    );
  });

  it('builds a ws URL and encodes gameId', () => {
    const original = environment.API_BASE_URL;
    // Absolute http URL ensures ws:// conversion
    (environment as any).API_BASE_URL = 'http://api.example.com';

    const conn = service.connect('abc 123');
    expect(conn).toBeTruthy();
    expect(capturedUrl).toBe('ws://api.example.com/games/abc%20123/live');

    // restore
    (environment as any).API_BASE_URL = original;
  });

  it('emits mapped streams for server messages', async () => {
    (environment as any).API_BASE_URL = 'http://api.example.com';
    const conn = service.connect('game');

    const gameStateP = firstValueFrom(conn.gameState$.pipe(take(1)));
    const playerJoinedP = firstValueFrom(conn.playerJoined$.pipe(take(1)));
    const errorP = firstValueFrom(conn.errors$.pipe(take(1)));

    // Emit messages
    fake.incoming$.next({
      type: 'game_state',
      game: {
        id: 'g1',
        playerLimit: 5,
        roundLimit: 10,
        players: [],
        rounds: [],
        state: 'IN_PROGRESS',
        finalGuess: null,
      },
    } as ServerMessage);

    fake.incoming$.next({
      type: 'player_joined',
      player: { name: 'Alice' },
    } as ServerMessage);

    fake.incoming$.next({ type: 'error', message: 'nope' } as ServerMessage);

    // Termination is covered by a dedicated test to avoid race conditions

    await expect(gameStateP).resolves.toEqual(
      expect.objectContaining({ id: 'g1' }),
    );
    await expect(playerJoinedP).resolves.toEqual(
      expect.objectContaining({ name: 'Alice' }),
    );
    await expect(errorP).resolves.toBe('nope');
  });

  it('submitGuess sends the correct payload', () => {
    (environment as any).API_BASE_URL = 'http://api.example.com';
    const conn = service.connect('game');

    conn.submitGuess('word');
    expect(fake.sent).toEqual(
      expect.arrayContaining([{ type: 'submit_guess', guess: 'word' }]),
    );
  });

  it('auto-closes the socket when terminated', async () => {
    (environment as any).API_BASE_URL = 'http://api.example.com';
    const conn = service.connect('game');
    const termP = firstValueFrom(conn.terminated$);

    expect(fake.completed).toBe(false);
    // Emulate server sending a completed game state
    fake.incoming$.next({
      type: 'game_state',
      game: {
        id: 'g2',
        playerLimit: 5,
        roundLimit: 10,
        players: [],
        rounds: [],
        state: 'COMPLETED',
        finalGuess: null,
      },
    } as ServerMessage);

    await expect(termP).resolves.toBe('COMPLETED');
    // Auto-close should have completed the fake socket
    expect(fake.completed).toBe(true);
  });

  it('auto-closes and emits TERMINATED state', async () => {
    (environment as any).API_BASE_URL = 'http://api.example.com';
    const conn = service.connect('game');
    const termP = firstValueFrom(conn.terminated$);

    fake.incoming$.next({
      type: 'game_state',
      game: {
        id: 'g3',
        playerLimit: 5,
        roundLimit: 10,
        players: [],
        rounds: [],
        state: 'TERMINATED',
        finalGuess: null,
      },
    } as ServerMessage);

    await expect(termP).resolves.toBe('TERMINATED');
    expect(fake.completed).toBe(true);
  });

  it('stops forwarding messages after close()', async () => {
    (environment as any).API_BASE_URL = 'http://api.example.com';
    const conn = service.connect('game');

    const received: ServerMessage[] = [];
    const sub = conn.messages$.subscribe((m) => received.push(m));

    fake.incoming$.next({ type: 'error', message: 'one' } as ServerMessage);
    expect(received.length).toBe(1);

    conn.close();

    fake.incoming$.next({ type: 'error', message: 'two' } as ServerMessage);
    // After close(), takeUntil should prevent further emissions to conn.messages$
    expect(received.length).toBe(1);

    sub.unsubscribe();
  });
});
