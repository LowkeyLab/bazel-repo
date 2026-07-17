import type { CreateSessionApiResponse } from "#shared/api";
import { createSession } from "../../utils/grpc";
import { observability } from "../../utils/observability";
import { setAuthCookie } from "../../utils/cookies";

function positiveInteger(value: string | undefined): number | undefined {
  if (!value || !/^\d+$/.test(value)) return undefined;
  const parsed = Number(value);
  return Number.isSafeInteger(parsed) && parsed > 0 ? parsed : undefined;
}

async function* requestChunks(
  request: NodeJS.ReadableStream,
): AsyncGenerator<Buffer> {
  for await (const chunk of request) {
    if (typeof chunk === "string") yield Buffer.from(chunk);
    else yield Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk);
  }
}

export default defineEventHandler(
  async (event): Promise<CreateSessionApiResponse> => {
    const config = useRuntimeConfig(event);
    const contentType = getRequestHeader(event, "content-type");
    const extension = getRequestHeader(
      event,
      "x-package-extension",
    )?.toLowerCase();
    const declaredSize = positiveInteger(
      getRequestHeader(event, "x-declared-size"),
    );
    const contentLength = positiveInteger(
      getRequestHeader(event, "content-length"),
    );
    if (
      contentType !== "application/octet-stream" ||
      extension !== ".apkg" ||
      !declaredSize ||
      declaredSize > Number(config.maxUploadBytes) ||
      (contentLength && contentLength !== declaredSize)
    ) {
      throw createError({ statusCode: 400, statusMessage: "invalid_upload" });
    }
    const requestId = crypto.randomUUID();
    const started = performance.now();
    observability().emit(
      "INFO",
      { requestId },
      { name: "upload.accepted", outcome: "success" },
    );
    const controller = new AbortController();
    event.node.req.once("aborted", () => controller.abort());
    try {
      const created = await createSession(
        requestChunks(event.node.req),
        extension,
        declaredSize,
        controller.signal,
      );
      setAuthCookie(
        event,
        created.token,
        Math.max(
          1,
          Math.floor((Date.parse(created.expiresAt) - Date.now()) / 1000),
        ),
      );
      observability().emit(
        "INFO",
        { requestId, sessionId: created.sessionId },
        {
          name: "upload.completed",
          outcome: "success",
          durationMs: Math.round(performance.now() - started),
          role: "test_taker",
        },
      );
      return {
        sessionId: created.sessionId,
        invitationPath: `/examine/${encodeURIComponent(created.sessionId)}#invite=${encodeURIComponent(created.invitation)}`,
        expiresAt: created.expiresAt,
        cardCount: created.cardCount,
        initialSnapshot: created.initialSnapshot,
      };
    } catch (error) {
      const code = error instanceof Error ? error.message : "upload_failed";
      observability().emit(
        "WARN",
        { requestId },
        {
          name: "upload.failed",
          outcome: controller.signal.aborted ? "cancelled" : "rejected",
          errorCode: code,
          durationMs: Math.round(performance.now() - started),
        },
      );
      throw createError({
        statusCode: controller.signal.aborted ? 499 : 422,
        statusMessage: code,
      });
    }
  },
);
