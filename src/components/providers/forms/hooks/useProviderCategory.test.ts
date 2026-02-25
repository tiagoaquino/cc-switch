import { renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { useProviderCategory } from "./useProviderCategory";

describe("useProviderCategory", () => {
  it("treats gemini oauth profile as official in edit mode", () => {
    const { result } = renderHook(() =>
      useProviderCategory({
        appId: "gemini",
        selectedPresetId: null,
        isEditMode: true,
        initialCategory: "custom",
        initialSettingsConfig: {
          env: {},
          config: {
            security: {
              auth: {
                selectedType: "oauth-personal",
              },
            },
          },
        },
      }),
    );

    expect(result.current.category).toBe("official");
  });

  it("keeps non-oauth gemini profile category in edit mode", () => {
    const { result } = renderHook(() =>
      useProviderCategory({
        appId: "gemini",
        selectedPresetId: null,
        isEditMode: true,
        initialCategory: "custom",
        initialSettingsConfig: {
          env: {
            GEMINI_API_KEY: "sk-123",
            GOOGLE_GEMINI_BASE_URL: "https://example.com",
          },
          config: {},
        },
      }),
    );

    expect(result.current.category).toBe("custom");
  });
});
