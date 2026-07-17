import {
  initializeObservability,
  shutdownObservability,
} from "../utils/observability";

export default defineNitroPlugin((nitroApp) => {
  const events = initializeObservability();
  events.emit("INFO", {}, { name: "service.started", outcome: "success" });
  events.emit("INFO", {}, { name: "service.ready", outcome: "success" });
  nitroApp.hooks.hookOnce("close", async () => {
    events.emit("INFO", {}, { name: "service.stopping", outcome: "success" });
    events.emit("INFO", {}, { name: "service.stopped", outcome: "success" });
    await shutdownObservability();
  });
});
