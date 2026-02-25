import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Provider } from "@/types";
import { EditProviderDialog } from "@/components/providers/EditProviderDialog";

const mockGetCurrent = vi.fn();
const mockGetLiveProviderSettings = vi.fn();

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string) => key,
  }),
}));

vi.mock("@/lib/api", () => ({
  providersApi: {
    getCurrent: (...args: unknown[]) => mockGetCurrent(...args),
  },
  vscodeApi: {
    getLiveProviderSettings: (...args: unknown[]) =>
      mockGetLiveProviderSettings(...args),
  },
}));

vi.mock("@/components/common/FullScreenPanel", () => ({
  FullScreenPanel: ({ children }: { children: any }) => (
    <div>{children}</div>
  ),
}));

vi.mock("@/components/ui/button", () => ({
  Button: ({ children }: { children: any }) => (
    <button type="button">{children}</button>
  ),
}));

vi.mock("@/components/providers/forms/ProviderForm", () => ({
  ProviderForm: ({
    initialData,
  }: {
    initialData?: { settingsConfig?: Record<string, unknown> };
  }) => {
    const env = (initialData?.settingsConfig?.env ?? {}) as Record<
      string,
      unknown
    >;
    return (
      <div data-testid="initial-env">
        {JSON.stringify(env)}
      </div>
    );
  },
}));

function buildProvider(settingsConfig: Record<string, unknown>): Provider {
  return {
    id: "p1",
    name: "Provider",
    settingsConfig,
  };
}

describe("EditProviderDialog", () => {
  beforeEach(() => {
    mockGetCurrent.mockReset();
    mockGetLiveProviderSettings.mockReset();
  });

  it("uses ssot settings for gemini edit instead of current live snapshot", async () => {
    const provider = buildProvider({
      env: {
        GOOGLE_CLOUD_PROJECT: "from-db",
      },
      config: {},
    });

    mockGetCurrent.mockResolvedValue("p1");
    mockGetLiveProviderSettings.mockResolvedValue({
      env: {},
      config: {},
    });

    render(
      <EditProviderDialog
        open={true}
        provider={provider}
        appId="gemini"
        onOpenChange={vi.fn()}
        onSubmit={vi.fn()}
      />,
    );

    await waitFor(() => {
      expect(screen.getByTestId("initial-env").textContent).toContain(
        "GOOGLE_CLOUD_PROJECT",
      );
      expect(screen.getByTestId("initial-env").textContent).toContain("from-db");
    });

    expect(mockGetCurrent).not.toHaveBeenCalled();
    expect(mockGetLiveProviderSettings).not.toHaveBeenCalled();
  });

  it("still reads live settings for claude current provider", async () => {
    const provider = buildProvider({
      env: {
        ANTHROPIC_BASE_URL: "https://from-db",
      },
      config: {},
    });

    mockGetCurrent.mockResolvedValue("p1");
    mockGetLiveProviderSettings.mockResolvedValue({
      env: {
        ANTHROPIC_BASE_URL: "https://from-live",
      },
      config: {},
    });

    render(
      <EditProviderDialog
        open={true}
        provider={provider}
        appId="claude"
        onOpenChange={vi.fn()}
        onSubmit={vi.fn()}
      />,
    );

    await waitFor(() => {
      expect(screen.getByTestId("initial-env").textContent).toContain(
        "https://from-live",
      );
    });

    expect(mockGetCurrent).toHaveBeenCalledWith("claude");
    expect(mockGetLiveProviderSettings).toHaveBeenCalledWith("claude");
  });
});
