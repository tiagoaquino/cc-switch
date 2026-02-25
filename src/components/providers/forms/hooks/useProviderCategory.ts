import { useState, useEffect } from "react";
import type { ProviderCategory } from "@/types";
import type { AppId } from "@/lib/api";
import { providerPresets } from "@/config/claudeProviderPresets";
import { codexProviderPresets } from "@/config/codexProviderPresets";
import { geminiProviderPresets } from "@/config/geminiProviderPresets";
import { opencodeProviderPresets } from "@/config/opencodeProviderPresets";

interface UseProviderCategoryProps {
  appId: AppId;
  selectedPresetId: string | null;
  isEditMode: boolean;
  initialCategory?: ProviderCategory;
  initialSettingsConfig?: Record<string, unknown>;
}

function isGeminiOauthOfficialSettings(
  settingsConfig?: Record<string, unknown>,
): boolean {
  if (!settingsConfig || typeof settingsConfig !== "object") {
    return false;
  }

  const configObj = settingsConfig.config as
    | Record<string, unknown>
    | undefined;
  const security = configObj?.security as Record<string, unknown> | undefined;
  const auth = security?.auth as Record<string, unknown> | undefined;
  const selectedType = auth?.selectedType;

  if (selectedType === "oauth-personal") {
    return true;
  }

  const authFiles = settingsConfig.authFiles as
    | Record<string, unknown>
    | undefined;
  const authFilesEnabled = authFiles?.enabled === true;
  if (!authFilesEnabled) {
    return false;
  }

  const env = settingsConfig.env as Record<string, unknown> | undefined;
  const geminiApiKey = env?.GEMINI_API_KEY;
  const googleApiKey = env?.GOOGLE_API_KEY;

  const hasGeminiApiKey =
    typeof geminiApiKey === "string" && geminiApiKey.trim() !== "";
  const hasGoogleApiKey =
    typeof googleApiKey === "string" && googleApiKey.trim() !== "";

  return !hasGeminiApiKey && !hasGoogleApiKey;
}

/**
 * 管理供应商类别状态
 * 根据选择的预设自动更新类别
 */
export function useProviderCategory({
  appId,
  selectedPresetId,
  isEditMode,
  initialCategory,
  initialSettingsConfig,
}: UseProviderCategoryProps) {
  const getEditModeCategory = () => {
    if (
      appId === "gemini" &&
      isGeminiOauthOfficialSettings(initialSettingsConfig)
    ) {
      return "official" as ProviderCategory;
    }

    return initialCategory;
  };

  const [category, setCategory] = useState<ProviderCategory | undefined>(
    // 编辑模式：使用 initialCategory
    isEditMode ? getEditModeCategory() : undefined,
  );

  useEffect(() => {
    // 编辑模式：只在初始化时设置，后续不自动更新
    if (isEditMode) {
      setCategory(getEditModeCategory());
      return;
    }

    if (selectedPresetId === "custom") {
      setCategory("custom");
      return;
    }

    if (!selectedPresetId) return;

    // 从预设 ID 提取索引
    const match = selectedPresetId.match(
      /^(claude|codex|gemini|opencode)-(\d+)$/,
    );
    if (!match) return;

    const [, type, indexStr] = match;
    const index = parseInt(indexStr, 10);

    if (type === "codex" && appId === "codex") {
      const preset = codexProviderPresets[index];
      if (preset) {
        setCategory(
          preset.category || (preset.isOfficial ? "official" : undefined),
        );
      }
    } else if (type === "claude" && appId === "claude") {
      const preset = providerPresets[index];
      if (preset) {
        setCategory(
          preset.category || (preset.isOfficial ? "official" : undefined),
        );
      }
    } else if (type === "gemini" && appId === "gemini") {
      const preset = geminiProviderPresets[index];
      if (preset) {
        setCategory(preset.category || undefined);
      }
    } else if (type === "opencode" && appId === "opencode") {
      const preset = opencodeProviderPresets[index];
      if (preset) {
        setCategory(preset.category || undefined);
      }
    }
  }, [
    appId,
    selectedPresetId,
    isEditMode,
    initialCategory,
    initialSettingsConfig,
  ]);

  return { category, setCategory };
}
