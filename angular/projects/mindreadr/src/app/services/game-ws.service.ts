import { Injectable } from '@angular/core';
import { environment } from '../../environments/environment';
import { Observable, Subject, map, shareReplay, takeUntil, filter } from 'rxjs';
import { webSocket, WebSocketSubject, WebSocketSubjectConfig } from 'rxjs/webSocket';

// Server -> Client messages (OutgoingMessage in Kotlin)
export type ServerMessage =
  | { type: 'game_state'; game: GameDto }
  | { type: 'game_terminated'; reason: string }
  | { type: 'error'; message: string }
  | { type: 'player_joined'; player: Player };

// Client -> Server messages (IncomingMessage in Kotlin)
export type ClientMessage = { type: 'submit_guess'; guess: string };

// Minimal types reflecting Kotlin DTOs
export type GameState = 'WAITING_FOR_PLAYERS' | 'IN_PROGRESS' | 'COMPLETED';

export interface Player {
  // Structure depends on server; keep flexible
  [key: string]: unknown;
}

export interface RoundDto {
  number: number;
  guesses: Record<string, string>;
}

export interface GameDto {
  id: string; // Kotlin value class likely serialized as string
  playerLimit: number;
  players: Player[];
  rounds: RoundDto[];
  state: GameState;
  finalGuess?: string | null;
}

export interface GameWsConnection {
  messages$: Observable<ServerMessage>;
  gameState$: Observable<GameDto>;
  playerJoined$: Observable<Player>;
  terminated$: Observable<string>; // reason
  errors$: Observable<string>; // message
  submitGuess: (guess: string) => void;
  close: () => void;
}

@Injectable({ providedIn: 'root' })
export class GameWsService {
  /** Create a new websocket connection for a specific game ID. */
  connect(gameId: string): GameWsConnection {
    const url = this.toWsUrl(
      `${environment.API_BASE_URL}/games/${encodeURIComponent(gameId)}/live`,
    );

    const destroy$ = new Subject<void>();

    const config: WebSocketSubjectConfig<ServerMessage | ClientMessage> = {
      url,
      serializer: (value: unknown) => JSON.stringify(value),
      deserializer: (e: MessageEvent) => JSON.parse(e.data),
      openObserver: undefined,
      closeObserver: undefined,
    };

    // Use a loosely typed socket for send flexibility, and narrow inbound stream separately
    const socket: WebSocketSubject<any> = this.createSocket(config as WebSocketSubjectConfig<any>);

    const incoming$ = socket.asObservable() as Observable<ServerMessage>;
    const messages$ = incoming$.pipe(
      takeUntil(destroy$),
      shareReplay({ bufferSize: 1, refCount: true }),
    );

    const gameState$ = messages$.pipe(
      filter((m): m is Extract<ServerMessage, { type: 'game_state' }> => m.type === 'game_state'),
      map((m) => m.game),
      shareReplay({ bufferSize: 1, refCount: true }),
    );

    const playerJoined$ = messages$.pipe(
      filter(
        (m): m is Extract<ServerMessage, { type: 'player_joined' }> => m.type === 'player_joined',
      ),
      map((m) => m.player),
    );

    const terminated$ = messages$.pipe(
      filter(
        (m): m is Extract<ServerMessage, { type: 'game_terminated' }> =>
          m.type === 'game_terminated',
      ),
      map((m) => m.reason),
      shareReplay({ bufferSize: 1, refCount: true }),
    );

    const errors$ = messages$.pipe(
      filter((m): m is Extract<ServerMessage, { type: 'error' }> => m.type === 'error'),
      map((m) => m.message),
    );

    const submitGuess = (guess: string) => {
      const payload: ClientMessage = { type: 'submit_guess', guess };
      socket.next(payload);
    };

    const close = () => {
      destroy$.next();
      destroy$.complete();
      try {
        socket.complete();
      } catch {
        // ignore
      }
    };

    // Auto-close on termination signal
    terminated$.pipe(takeUntil(destroy$)).subscribe(() => close());

    return { messages$, gameState$, playerJoined$, terminated$, errors$, submitGuess, close };
  }

  private toWsUrl(httpish: string): string {
    if (httpish.startsWith('ws://') || httpish.startsWith('wss://')) return httpish;
    if (httpish.startsWith('https://')) return httpish.replace(/^https:\/\//, 'wss://');
    if (httpish.startsWith('http://')) return httpish.replace(/^http:\/\//, 'ws://');
    // Fallback: treat as relative to current origin
    const isSecure = window.location.protocol === 'https:';
    const base = `${isSecure ? 'wss' : 'ws'}://${window.location.host}`;
    return `${base}${httpish.startsWith('/') ? '' : '/'}${httpish}`;
  }

  private createSocket<T = any>(config: WebSocketSubjectConfig<T>): WebSocketSubject<T> {
    return webSocket(config as WebSocketSubjectConfig<T>);
  }
}
