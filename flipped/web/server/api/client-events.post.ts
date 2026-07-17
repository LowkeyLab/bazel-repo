import type { ServiceEvent, ServiceEventName } from "#shared/observability";
import { observability } from "../utils/observability";

const allowed = new Set<ServiceEventName>(["client.connection_changed"]);
const allowedErrorCodes = new Set([
  "command_timeout",
  "connection_failed",
  "disconnected",
  "origin_configuration_invalid",
  "origin_missing",
  "origin_rejected",
  "socket_capacity_exhausted",
  "unauthenticated",
]);
let windowStart = 0;
let accepted = 0;

export default defineEventHandler(async (event) => {
  const contentLength = Number(getRequestHeader(event, "content-length"));
  if (
    !Number.isSafeInteger(contentLength) ||
    contentLength <= 0 ||
    contentLength > 4_096
  )
    throw createError({ statusCode: 413, statusMessage: "invalid_event_size" });
  const now = Date.now();
  if (now - windowStart >= 60_000) {
    windowStart = now;
    accepted = 0;
  }
  if (++accepted > 1_000)
    throw createError({ statusCode: 429, statusMessage: "rate_limited" });
  const body = await readBody<Partial<ServiceEvent>>(event);
  if (
    !body.name ||
    !allowed.has(body.name) ||
    !["success", "failure", "cancelled", "rejected"].includes(
      body.outcome ?? "",
    ) ||
    (body.role !== undefined &&
      !["test_taker", "examiner"].includes(body.role)) ||
    (body.errorCode !== undefined && !allowedErrorCodes.has(body.errorCode))
  ) {
    throw createError({ statusCode: 400, statusMessage: "invalid_event" });
  }
  observability().emit(
    body.outcome === "failure" ? "WARN" : "INFO",
    {},
    {
      name: body.name,
      outcome: body.outcome as ServiceEvent["outcome"],
      ...(body.errorCode ? { errorCode: body.errorCode } : {}),
      ...(body.role ? { role: body.role } : {}),
    },
  );
  return { accepted: true };
});
