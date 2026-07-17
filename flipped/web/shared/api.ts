import type { TestTakerSnapshot } from "./session";

export interface CreateSessionApiResponse {
  readonly sessionId: string;
  readonly invitationPath: string;
  readonly expiresAt: string;
  readonly cardCount: number;
  readonly initialSnapshot: TestTakerSnapshot;
}

export interface RedeemInvitationApiResponse {
  readonly sessionId: string;
  readonly expiresIn: number;
}
