// Shared Game DTO types aligned with Kotlin's GameDto
// Keep minimal structure to match backend serialization.

export type GameState =
  | 'WAITING_FOR_PLAYERS'
  | 'IN_PROGRESS'
  | 'COMPLETED'
  | 'TERMINATED';

export interface Player {
  name: string;
}

export interface RoundDto {
  number: number;
  guesses: Record<string, string>;
}

export interface GameDto {
  id: string;
  playerLimit: number;
  roundLimit: number;
  players: Player[];
  rounds: RoundDto[];
  state: GameState;
  finalGuess?: string | null;
}
