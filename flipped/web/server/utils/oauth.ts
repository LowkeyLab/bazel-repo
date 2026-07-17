import * as oauth from "oauth4webapi";

export interface ExchangedToken {
  readonly accessToken: string;
  readonly expiresIn: number;
}

export async function exchangeInvitation(
  invitation: string,
  redemptionId: string,
): Promise<ExchangedToken> {
  const config = useRuntimeConfig();
  const issuer = new URL(String(config.oauthIssuer));
  const as: oauth.AuthorizationServer = {
    issuer: issuer.href.replace(/\/$/, ""),
    token_endpoint: new URL("/oauth/token", issuer).href,
  };
  const client: oauth.Client = { client_id: String(config.oauthClientId) };
  const parameters = new URLSearchParams({
    subject_token: invitation,
    subject_token_type:
      "urn:flipped:params:oauth:token-type:examiner-invitation",
    requested_token_type: "urn:ietf:params:oauth:token-type:access_token",
    audience: String(config.oauthAudience),
    scope: "session:examine",
    flipped_redemption_id: redemptionId,
  });
  const options =
    issuer.protocol === "http:"
      ? { [oauth.allowInsecureRequests]: true }
      : undefined;
  const response = await oauth.genericTokenEndpointRequest(
    as,
    client,
    oauth.ClientSecretBasic(String(config.oauthClientSecret)),
    "urn:ietf:params:oauth:grant-type:token-exchange",
    parameters,
    options,
  );
  const result = await oauth.processGenericTokenEndpointResponse(
    as,
    client,
    response,
  );
  if ("error" in result)
    throw new Error(
      typeof result.error === "string" ? result.error : "oauth_error",
    );
  if (
    typeof result.access_token !== "string" ||
    typeof result.expires_in !== "number"
  )
    throw new Error("invalid_token_response");
  return { accessToken: result.access_token, expiresIn: result.expires_in };
}
