import type { H3Event } from "h3";
import { deleteCookie, getCookie, setCookie } from "h3";

export function secureCookie(value: unknown): boolean {
  return value === true || value === "true";
}

export function authCookieName(secure: boolean): string {
  return secure ? "__Host-flipped-auth" : "flipped-auth";
}

export function readAuthCookie(event: H3Event): string | undefined {
  const config = useRuntimeConfig(event);
  return getCookie(event, authCookieName(secureCookie(config.cookieSecure)));
}

export function setAuthCookie(
  event: H3Event,
  token: string,
  maxAge: number,
): void {
  const config = useRuntimeConfig(event);
  const secure = secureCookie(config.cookieSecure);
  setCookie(event, authCookieName(secure), token, {
    httpOnly: true,
    secure,
    sameSite: "lax",
    path: "/",
    maxAge,
  });
}

export function clearAuthCookie(event: H3Event): void {
  const config = useRuntimeConfig(event);
  const secure = secureCookie(config.cookieSecure);
  deleteCookie(event, authCookieName(secure), {
    path: "/",
    secure,
  });
}

export function readCookieHeader(
  header: string | undefined,
  name: string,
): string | undefined {
  if (!header) return undefined;
  for (const part of header.split(";")) {
    const separator = part.indexOf("=");
    if (separator < 0) continue;
    const key = part.slice(0, separator).trim();
    if (key !== name) continue;
    try {
      return decodeURIComponent(part.slice(separator + 1).trim());
    } catch {
      return undefined;
    }
  }
  return undefined;
}
