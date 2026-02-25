import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AppId } from "@/lib/api";
import App from "./App";
import { server } from "../tests/msw/server";

vi.mock("@/components/UpdateBadge", () => ({
  UpdateBadge: () => null,
}));

const toastSuccessMock = vi.fn();
const toastErrorMock = vi.fn();

vi.mock("sonner", () => ({
  toast: {
    success: (...args: unknown[]) => toastSuccessMock(...args),
    error: (...args: unknown[]) => toastErrorMock(...args),
  },
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

describe("App logout context flow", () => {
  beforeEach(() => {
    localStorage.clear();
    sessionStorage.clear();
    toastSuccessMock.mockReset();
    toastErrorMock.mockReset();
  });

  it("runs logout context command and shows success toast", async () => {
    await renderAppFor("gemini");
    const user = userEvent.setup();

    await user.click(screen.getByTitle("provider.logoutContextTooltip"));
    await user.click(screen.getByRole("button", { name: "common.confirm" }));

    await waitFor(() => {
      expect(toastSuccessMock).toHaveBeenCalled();
    });
  });

  it("shows error toast when logout context command fails", async () => {
    server.use(
      http.post("http://tauri.local/logout_provider_context", () =>
        HttpResponse.text("logout failed", { status: 500 }),
      ),
    );

    await renderAppFor("claude");
    const user = userEvent.setup();

    await user.click(screen.getByTitle("provider.logoutContextTooltip"));
    await user.click(screen.getByRole("button", { name: "common.confirm" }));

    await waitFor(() => {
      expect(toastErrorMock).toHaveBeenCalled();
    });
  });
});
