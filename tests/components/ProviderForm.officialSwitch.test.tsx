import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ProviderForm } from "@/components/providers/forms/ProviderForm";
import type { ProviderCategory } from "@/types";

const toastError = vi.fn();

vi.mock("sonner", () => ({
  toast: {
    error: (...args: unknown[]) => toastError(...args),
    success: vi.fn(),
    warning: vi.fn(),
    info: vi.fn(),
  },
}));

vi.mock("@/components/providers/forms/CodexConfigEditor", () => ({
  default: () => <div data-testid="codex-config-editor" />,
}));

vi.mock("@/components/providers/forms/GeminiConfigEditor", () => ({
  default: () => <div data-testid="gemini-config-editor" />,
}));

vi.mock("@/components/JsonEditor", () => ({
  default: () => <div data-testid="json-editor" />,
}));

vi.mock("@/components/providers/forms/ProviderAdvancedConfig", () => ({
  ProviderAdvancedConfig: () => <div data-testid="provider-advanced-config" />,
}));

vi.mock("@/components/providers/forms/ClaudeQuickToggles", () => ({
  ClaudeQuickToggles: () => null,
  jsonMergePatch: vi.fn(),
}));

vi.mock("@/components/providers/forms/OpenCodeFormFields", () => ({
  OpenCodeFormFields: () => <div data-testid="opencode-fields" />,
}));

vi.mock("@/components/providers/forms/OpenClawFormFields", () => ({
  OpenClawFormFields: () => <div data-testid="openclaw-fields" />,
}));

vi.mock("@/components/providers/forms/OmoFormFields", () => ({
  OmoFormFields: () => <div data-testid="omo-fields" />,
}));

vi.mock("@/components/providers/forms/OmoCommonConfigEditor", () => ({
  OmoCommonConfigEditor: () => <div data-testid="omo-common-config-editor" />,
}));

function renderProviderForm({
  appId,
  initialCategory,
  initialSettingsConfig,
  onSubmit = vi.fn(),
}: {
  appId: "claude" | "codex" | "gemini";
  initialCategory?: ProviderCategory;
  initialSettingsConfig?: Record<string, unknown>;
  onSubmit?: ReturnType<typeof vi.fn>;
}) {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });

  const initialData =
    initialSettingsConfig !== undefined || initialCategory !== undefined
      ? {
          name: "Imported Config",
          websiteUrl: "",
          settingsConfig: initialSettingsConfig,
          category: initialCategory,
        }
      : undefined;

  const utils = render(
    <QueryClientProvider client={queryClient}>
      <ProviderForm
        appId={appId}
        submitLabel="save"
        onSubmit={onSubmit}
        onCancel={vi.fn()}
        initialData={initialData}
      />
    </QueryClientProvider>,
  );

  return {
    ...utils,
    onSubmit,
  };
}

function getOfficialSwitch() {
  return screen.getByRole("switch");
}

const endpointLabelMatcher = /providerForm\.apiEndpoint|API 端点|API Endpoint|API エンドポイント/i;

describe("ProviderForm official API switch", () => {
  it("shows switch in add mode for custom/official and hides it for third-party/aggregator/cn_official presets", async () => {
    const codexView = renderProviderForm({ appId: "codex" });

    expect(
      screen.getByText("providerForm.officialApiSwitchLabel"),
    ).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /OpenAI Official/i }));
    expect(
      screen.getByText("providerForm.officialApiSwitchLabel"),
    ).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /Azure OpenAI/i }));
    await waitFor(() => {
      expect(
        screen.queryByText("providerForm.officialApiSwitchLabel"),
      ).not.toBeInTheDocument();
    });

    codexView.unmount();

    renderProviderForm({ appId: "claude" });
    expect(
      screen.getByText("providerForm.officialApiSwitchLabel"),
    ).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /DeepSeek/i }));
    await waitFor(() => {
      expect(
        screen.queryByText("providerForm.officialApiSwitchLabel"),
      ).not.toBeInTheDocument();
    });

    fireEvent.click(screen.getByRole("button", { name: /ModelScope/i }));
    await waitFor(() => {
      expect(
        screen.queryByText("providerForm.officialApiSwitchLabel"),
      ).not.toBeInTheDocument();
    });
  });

  it("shows switch in edit mode for custom/official/undefined categories and hides for third-party", () => {
    const codexSettings = {
      auth: { OPENAI_API_KEY: "sk-test" },
      config:
        'model_provider = "openai"\nmodel = "gpt-5-codex"\n[model_providers.openai]\nbase_url = "https://api.example.com/v1"\nrequires_openai_auth = true',
    };

    const customView = renderProviderForm({
      appId: "codex",
      initialCategory: "custom",
      initialSettingsConfig: codexSettings,
    });
    expect(
      screen.getByText("providerForm.officialApiSwitchLabel"),
    ).toBeInTheDocument();
    customView.unmount();

    const officialView = renderProviderForm({
      appId: "codex",
      initialCategory: "official",
      initialSettingsConfig: codexSettings,
    });
    expect(
      screen.getByText("providerForm.officialApiSwitchLabel"),
    ).toBeInTheDocument();
    officialView.unmount();

    const undefinedView = renderProviderForm({
      appId: "codex",
      initialCategory: undefined,
      initialSettingsConfig: codexSettings,
    });
    expect(
      screen.getByText("providerForm.officialApiSwitchLabel"),
    ).toBeInTheDocument();
    undefinedView.unmount();

    renderProviderForm({
      appId: "codex",
      initialCategory: "third_party",
      initialSettingsConfig: codexSettings,
    });
    expect(
      screen.queryByText("providerForm.officialApiSwitchLabel"),
    ).not.toBeInTheDocument();
  });

  it("blocks Codex save without endpoint when switch is off and allows save when switch is on", async () => {
    toastError.mockClear();
    const onSubmit = vi.fn();

    renderProviderForm({
      appId: "codex",
      initialCategory: "custom",
      initialSettingsConfig: {
        auth: { OPENAI_API_KEY: "sk-test" },
        config: 'model_provider = "openai"\nmodel = "gpt-5-codex"',
      },
      onSubmit,
    });

    expect(screen.getByText("codexConfig.apiUrlLabel")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "save" }));
    await waitFor(() => {
      expect(onSubmit).not.toHaveBeenCalled();
      expect(toastError).toHaveBeenCalled();
    });
    expect(String(toastError.mock.calls[0]?.[0] ?? "")).toMatch(
      /endpoint|端点|エンドポイント|providerForm\.endpointRequired/i,
    );

    fireEvent.click(getOfficialSwitch());

    await waitFor(() => {
      expect(screen.queryByText("codexConfig.apiUrlLabel")).not.toBeInTheDocument();
    });

    fireEvent.click(screen.getByRole("button", { name: "save" }));
    await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(1));
    expect(onSubmit.mock.calls[0][0].presetCategory).toBe("official");
  });

  it("saves official profile as custom when switch is turned off", async () => {
    const onSubmit = vi.fn();

    renderProviderForm({
      appId: "codex",
      initialCategory: "official",
      initialSettingsConfig: {
        auth: { OPENAI_API_KEY: "sk-test" },
        config:
          'model_provider = "openai"\nmodel = "gpt-5-codex"\n[model_providers.openai]\nbase_url = "https://api.example.com/v1"\nrequires_openai_auth = true',
      },
      onSubmit,
    });

    expect(getOfficialSwitch()).toHaveAttribute("data-state", "checked");
    fireEvent.click(getOfficialSwitch());

    fireEvent.click(screen.getByRole("button", { name: "save" }));
    await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(1));
    expect(onSubmit.mock.calls[0][0].presetCategory).toBe("custom");
  });

  it("applies switch behavior for Claude and Gemini (hide endpoint + submit as official)", async () => {
    const claudeSubmit = vi.fn();
    const { unmount } = renderProviderForm({
      appId: "claude",
      initialCategory: "custom",
      initialSettingsConfig: {
        env: {
          ANTHROPIC_AUTH_TOKEN: "sk-claude",
        },
      },
      onSubmit: claudeSubmit,
    });

    expect(screen.getByText(endpointLabelMatcher)).toBeInTheDocument();
    fireEvent.click(getOfficialSwitch());
    await waitFor(() => {
      expect(screen.queryByText(endpointLabelMatcher)).not.toBeInTheDocument();
    });
    fireEvent.click(screen.getByRole("button", { name: "save" }));
    await waitFor(() => expect(claudeSubmit).toHaveBeenCalledTimes(1));
    expect(claudeSubmit.mock.calls[0][0].presetCategory).toBe("official");
    unmount();

    const geminiSubmit = vi.fn();
    renderProviderForm({
      appId: "gemini",
      initialCategory: "custom",
      initialSettingsConfig: {
        env: {
          GEMINI_API_KEY: "sk-gemini",
          GEMINI_MODEL: "gemini-3-pro",
        },
        config: {},
      },
      onSubmit: geminiSubmit,
    });

    expect(screen.getByText(endpointLabelMatcher)).toBeInTheDocument();
    fireEvent.click(getOfficialSwitch());
    await waitFor(() => {
      expect(screen.queryByText(endpointLabelMatcher)).not.toBeInTheDocument();
    });
    fireEvent.click(screen.getByRole("button", { name: "save" }));
    await waitFor(() => expect(geminiSubmit).toHaveBeenCalledTimes(1));
    expect(geminiSubmit.mock.calls[0][0].presetCategory).toBe("official");
  });
});
