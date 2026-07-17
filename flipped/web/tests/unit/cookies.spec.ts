import { describe, expect, it } from "vitest";
import {
  authCookieName,
  readCookieHeader,
  secureCookie,
} from "../../server/utils/cookies";

describe("socket cookie parsing", () => {
  it("parses boolean runtime overrides without treating false as truthy", () => {
    expect(secureCookie(true)).toBe(true);
    expect(secureCookie("true")).toBe(true);
    expect(secureCookie(false)).toBe(false);
    expect(secureCookie("false")).toBe(false);
    expect(authCookieName(false)).toBe("flipped-auth");
  });

  it("extracts only the exact encoded cookie and rejects malformed encoding", () => {
    expect(
      readCookieHeader("other=x; flipped-auth=a%2Eb%2Ec", "flipped-auth"),
    ).toBe("a.b.c");
    expect(
      readCookieHeader("flipped-authx=wrong", "flipped-auth"),
    ).toBeUndefined();
    expect(
      readCookieHeader("flipped-auth=%GG", "flipped-auth"),
    ).toBeUndefined();
  });
});
