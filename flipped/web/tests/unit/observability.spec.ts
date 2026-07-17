import { describe, expect, it } from "vitest";
import type { EventEnvelope } from "../../shared/observability";
import {
  EventDispatcher,
  type EventListener,
} from "../../server/utils/observability";

class RecordingListener implements EventListener {
  readonly events: EventEnvelope[] = [];
  onEvent(event: EventEnvelope): void {
    this.events.push(event);
  }
}

describe("EventDispatcher", () => {
  it("emits a typed pseudonymous envelope and isolates failed listeners", () => {
    const recording = new RecordingListener();
    const failing: EventListener = {
      onEvent: () => {
        throw new Error("failure");
      },
    };
    const dispatcher = new EventDispatcher(
      {
        name: "flipped-web",
        version: "test",
        environment: "test",
        instanceId: "instance",
      },
      Buffer.alloc(32, 7),
      [failing, recording],
    );
    dispatcher.emit(
      "INFO",
      { sessionId: "raw-session" },
      { name: "socket.connected", outcome: "success" },
    );
    expect(recording.events).toHaveLength(1);
    expect(recording.events[0]?.sessionRef).toBeTruthy();
    expect(JSON.stringify(recording.events[0])).not.toContain("raw-session");
    expect(dispatcher.droppedEvents()).toBe(1);
  });
});
