import {
  v7 as uuidv7,
  validate as validateUuid,
  version as uuidVersion,
} from "uuid";
import type { RedeemInvitationApiResponse } from "#shared/api";
import { exchangeInvitation } from "../../../../utils/oauth";
import { getSnapshot } from "../../../../utils/grpc";
import { observability } from "../../../../utils/observability";
import { setAuthCookie } from "../../../../utils/cookies";

interface RedeemBody {
  invitation?: unknown;
  redemptionId?: unknown;
}

export default defineEventHandler(
  async (event): Promise<RedeemInvitationApiResponse> => {
    const sessionId = getRouterParam(event, "id");
    const body = await readBody<RedeemBody>(event);
    if (
      !sessionId ||
      typeof body.invitation !== "string" ||
      body.invitation.length < 32 ||
      body.invitation.length > 512
    ) {
      throw createError({ statusCode: 400, statusMessage: "invalid_request" });
    }
    const redemptionId =
      typeof body.redemptionId === "string" ? body.redemptionId : uuidv7();
    if (!validateUuid(redemptionId) || uuidVersion(redemptionId) !== 7)
      throw createError({ statusCode: 400, statusMessage: "invalid_request" });
    const requestId = crypto.randomUUID();
    observability().emit(
      "INFO",
      { requestId, sessionId },
      {
        name: "oauth.exchange_requested",
        outcome: "success",
        role: "examiner",
      },
    );
    try {
      const exchanged = await exchangeInvitation(body.invitation, redemptionId);
      await getSnapshot(sessionId, "examiner", exchanged.accessToken);
      setAuthCookie(event, exchanged.accessToken, exchanged.expiresIn);
      observability().emit(
        "INFO",
        { requestId, sessionId },
        {
          name: "oauth.exchange_succeeded",
          outcome: "success",
          role: "examiner",
        },
      );
      return { sessionId, expiresIn: exchanged.expiresIn };
    } catch (error) {
      const code = error instanceof Error ? error.message : "invalid_grant";
      observability().emit(
        "WARN",
        { requestId, sessionId },
        {
          name: "oauth.exchange_rejected",
          outcome: "rejected",
          errorCode: code,
          role: "examiner",
        },
      );
      throw createError({
        statusCode: code === "invalid_grant" ? 400 : 502,
        statusMessage: code,
      });
    }
  },
);
