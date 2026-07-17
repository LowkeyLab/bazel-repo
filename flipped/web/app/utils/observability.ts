import type { ServiceEvent } from "#shared/observability";

export interface ClientEventListener {
  onEvent(event: ServiceEvent): void;
}

class BrowserReportListener implements ClientEventListener {
  onEvent(event: ServiceEvent): void {
    void $fetch("/api/client-events", { method: "POST", body: event }).catch(
      () => undefined,
    );
  }
}

const listener: ClientEventListener = new BrowserReportListener();

export function emitClientEvent(event: ServiceEvent): void {
  listener.onEvent(event);
}
