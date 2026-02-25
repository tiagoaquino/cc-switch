import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AppId } from "@/lib/api";
import App from "./App";

vi.mock("@/components/UpdateBadge", () => ({
  UpdateBadge: () => null,
}));

const renderAppFor = async (appId: AppId) => {
  localStorage.setItem("cc-switch-last-app", appId);
  localStorage.setItem("cc-switch-last-view", "providers");

  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });

  render(
    <QueryClientProvider client={queryClient}>
      <App />
    </QueryClientProvider>,
  );

  await waitFor(() => {
    expect(screen.getByText("CC Switch")).toBeInTheDocument();
  });
};

describe("App import current config button", () => {
  beforeEach(() => {
    localStorage.clear();
    sessionStorage.clear();
  });

  it.each(["claude", "codex", "gemini"] as const)(
    "shows header import button for %s",
    async (appId) => {
      await renderAppFor(appId);
      expect(screen.getByTitle("provider.importCurrent")).toBeInTheDocument();
    },
  );

  it.each(["opencode", "openclaw"] as const)(
    "does not show header import button for %s",
    async (appId) => {
      await renderAppFor(appId);
      expect(
        screen.queryByTitle("provider.importCurrent"),
      ).not.toBeInTheDocument();
    },
  );

  it.each(["claude", "codex", "gemini", "opencode", "openclaw"] as const)(
    "shows header logout context button for %s",
    async (appId) => {
      await renderAppFor(appId);
      expect(screen.getByTitle("provider.logoutContextTooltip")).toBeInTheDocument();
    },
  );
});
