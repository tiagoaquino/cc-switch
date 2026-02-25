import { describe, expect, it } from "vitest";
import { buildGeminiSettingsConfig } from "./geminiSettings";

describe("buildGeminiSettingsConfig", () => {
  it("excludes authFiles when manageAuthFiles is false", () => {
    const result = buildGeminiSettingsConfig({
      env: { GEMINI_API_KEY: "k1" },
      config: {},
      manageAuthFiles: false,
      googleAccounts: { accounts: [] },
      oauthCreds: { refresh_token: "rt" },
    });

    expect(result.authFiles).toBeUndefined();
  });

  it("includes authFiles when manageAuthFiles is true", () => {
    const result = buildGeminiSettingsConfig({
      env: {},
      config: {},
      manageAuthFiles: true,
      googleAccounts: { accounts: [{ id: "a1" }] },
      oauthCreds: { refresh_token: "rt1" },
    });

    expect(result.authFiles).toEqual({
      enabled: true,
      googleAccounts: { accounts: [{ id: "a1" }] },
      oauthCreds: { refresh_token: "rt1" },
    });
  });
});
