// @vitest-environment jsdom
import { createPinia, setActivePinia } from "pinia";
import { defineComponent, h, nextTick } from "vue";
import { mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

const harness = vi.hoisted(() => {
  const handlers = new Map<string, (...arguments_: unknown[]) => void>();
  const socket = {
    on: vi.fn((event: string, handler: (...arguments_: unknown[]) => void) => {
      handlers.set(event, handler);
      return socket;
    }),
    emit: vi.fn((event: string, ...arguments_: unknown[]) => {
      if (event === "session:join") {
        const acknowledge = arguments_.at(-1);
        if (typeof acknowledge === "function") acknowledge({ ok: true });
      }
      return socket;
    }),
    connect: vi.fn(() => {
      handlers.get("connect")?.();
      return socket;
    }),
    disconnect: vi.fn(() => socket),
    removeAllListeners: vi.fn(() => socket),
  };
  return { handlers, socket };
});

vi.mock("socket.io-client", () => ({ io: () => harness.socket }));

import { useSessionSocket } from "../../app/composables/useSessionSocket.client";
import { useExaminerSessionStore } from "../../app/stores/examiner";

describe("useSessionSocket", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    harness.handlers.clear();
    vi.clearAllMocks();
    vi.stubGlobal(
      "$fetch",
      vi.fn(() => Promise.resolve()),
    );
    vi.useFakeTimers();
  });

  it("reduces snapshots and resynchronizes after an ambiguous command timeout", async () => {
    let commands: ReturnType<typeof useSessionSocket> | undefined;
    const host = defineComponent({
      setup() {
        commands = useSessionSocket("examiner", "session");
        return () => h("div");
      },
    });
    const wrapper = mount(host);
    await nextTick();
    harness.handlers.get("session:snapshot")?.({
      role: "examiner",
      sessionId: "session",
      revision: 1,
      status: "ready",
      testTakerConnected: true,
      expiresAt: "2030-01-01T00:00:00.000Z",
    });
    expect(useExaminerSessionStore().status).toBe("ready");

    commands?.start();
    expect(useExaminerSessionStore().pendingCommandId).toBeTruthy();
    await vi.advanceTimersByTimeAsync(10_000);
    expect(useExaminerSessionStore().errorCode).toBe("command_timeout");
    expect(harness.socket.disconnect).toHaveBeenCalledOnce();
    expect(harness.socket.connect).toHaveBeenCalledTimes(2);

    wrapper.unmount();
    vi.useRealTimers();
  });
});
