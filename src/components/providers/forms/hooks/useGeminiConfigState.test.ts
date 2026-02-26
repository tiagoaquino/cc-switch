import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { useGeminiConfigState } from "./useGeminiConfigState";

describe("useGeminiConfigState", () => {
  it("initializes authFiles state from settingsConfig", () => {
    const { result } = renderHook(() =>
      useGeminiConfigState({
        initialData: {
          settingsConfig: {
            env: {},
            config: {},
            authFiles: {
              enabled: true,
              googleAccounts: { accounts: [{ id: "a1" }] },
              oauthCreds: { refresh_token: "rt1" },
            },
          },
        },
      }),
    );

    expect(result.current.manageAuthFiles).toBe(true);
    expect(result.current.googleAccountsJson).toContain('"a1"');
    expect(result.current.oauthCredsJson).toContain('"rt1"');
  });

  it("validates oauth files json content independently", () => {
    const { result } = renderHook(() => useGeminiConfigState({}));

    act(() => {
      result.current.handleGoogleAccountsJsonChange("{invalid");
      result.current.handleOauthCredsJsonChange("{}");
    });

    expect(result.current.googleAccountsError).toBe("Invalid JSON format");
    expect(result.current.oauthCredsError).toBe("");
  });

  it("resets api/base/model state when env keys are removed on reinitialize", () => {
    const { result, rerender } = renderHook(
      ({ initialData }: { initialData?: { settingsConfig?: Record<string, unknown> } }) =>
        useGeminiConfigState({ initialData }),
      {
        initialProps: {
          initialData: {
            settingsConfig: {
              env: {
                GEMINI_API_KEY: "sk-test",
                GOOGLE_GEMINI_BASE_URL: "https://example.com",
                GEMINI_MODEL: "gemini-3-pro-preview",
              } as Record<string, unknown>,
              config: {},
            },
          },
        },
      },
    );

    expect(result.current.geminiApiKey).toBe("sk-test");
    expect(result.current.geminiBaseUrl).toBe("https://example.com");
    expect(result.current.geminiModel).toBe("gemini-3-pro-preview");

    rerender({
      initialData: {
        settingsConfig: {
          env: {} as Record<string, unknown>,
          config: {},
        },
      },
    });

    expect(result.current.geminiApiKey).toBe("");
    expect(result.current.geminiBaseUrl).toBe("");
    expect(result.current.geminiModel).toBe("");
  });
});
