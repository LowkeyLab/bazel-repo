import { describe, expect, it } from "vitest";
import {
  parseCommandResponse,
  parseCreateResponse,
  parseWatchResponse,
} from "../../server/utils/protocol";

const timestamp = { seconds: "1893456000", nanos: 0 };
const examinerSnapshot = {
  sessionId: "session",
  revision: "2",
  status: "IN_PROGRESS",
  testTakerConnected: true,
  currentCard: {
    cardId: "card",
    position: 1,
    total: 2,
    front: "front",
    back: "back",
  },
  expiresAt: timestamp,
};

describe("gRPC protocol validation", () => {
  it("parses discriminated create success without exposing transport details", () => {
    const result = parseCreateResponse({
      result: "success",
      success: {
        sessionId: "session",
        testTakerAccessToken: "token",
        examinerInvitation: "invitation",
        expiresAt: timestamp,
        cardCount: 2,
        initialSnapshot: {
          sessionId: "session",
          revision: "1",
          status: "WAITING_FOR_EXAMINER",
          examinerConnected: false,
          expiresAt: timestamp,
        },
      },
    });
    expect(result.initialSnapshot.role).toBe("test_taker");
    expect(result.cardCount).toBe(2);
  });

  it("parses role-specific stream projections without leaking card backs", () => {
    expect(
      parseCommandResponse({
        result: "success",
        success: { snapshot: examinerSnapshot },
      }).currentCard?.back,
    ).toBe("back");
    const examiner = parseWatchResponse(
      {
        result: "event",
        event: {
          revision: "3",
          payload: "cardChanged",
          cardChanged: { currentCard: examinerSnapshot.currentCard },
        },
      },
      "examiner",
    );
    expect(examiner.update).toMatchObject({
      kind: "card_changed",
      revision: 3,
    });
    const testTaker = parseWatchResponse(
      {
        result: "snapshot",
        snapshot: {
          sessionId: "session",
          revision: "3",
          status: "IN_PROGRESS",
          examinerConnected: true,
          currentCard: examinerSnapshot.currentCard,
          expiresAt: timestamp,
        },
      },
      "test_taker",
    );
    expect(testTaker.snapshot?.currentCard).toEqual({
      cardId: "card",
      position: 1,
      total: 2,
      front: "front",
    });
  });

  it("rejects missing and malformed discriminants", () => {
    expect(() =>
      parseCommandResponse({ success: { snapshot: examinerSnapshot } }),
    ).toThrow("invalid_protocol_result");
    expect(() =>
      parseWatchResponse(
        { result: "event", event: { revision: "x", payload: "ended" } },
        "examiner",
      ),
    ).toThrow("invalid_protocol_message");
  });
});
