import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactElement } from "react";
import { describe, expect, it, vi } from "vitest";
import { ProviderList } from "./ProviderList";

const renderWithQueryClient = (ui: ReactElement) => {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });

  return render(
    <QueryClientProvider client={queryClient}>{ui}</QueryClientProvider>,
  );
};

describe("ProviderList import entrypoint", () => {
  it("uses onImport callback in empty state", async () => {
    const onImport = vi.fn();
    const user = userEvent.setup();

    renderWithQueryClient(
      <ProviderList
        providers={{}}
        currentProviderId=""
        appId="claude"
        onSwitch={() => undefined}
        onEdit={() => undefined}
        onDelete={() => undefined}
        onDuplicate={() => undefined}
        onOpenWebsite={() => undefined}
        onCreate={() => undefined}
        onImport={onImport}
      />,
    );

    await user.click(
      screen.getByRole("button", { name: "provider.importCurrent" }),
    );
    expect(onImport).toHaveBeenCalledTimes(1);
  });

  it("does not render import button in non-empty list", () => {
    renderWithQueryClient(
      <ProviderList
        providers={{
          p1: {
            id: "p1",
            name: "Provider 1",
            settingsConfig: {},
            createdAt: Date.now(),
          },
        }}
        currentProviderId="p1"
        appId="claude"
        onSwitch={() => undefined}
        onEdit={() => undefined}
        onDelete={() => undefined}
        onDuplicate={() => undefined}
        onOpenWebsite={() => undefined}
        onCreate={() => undefined}
        onImport={() => undefined}
      />,
    );

    expect(
      screen.queryByRole("button", { name: "provider.importCurrent" }),
    ).not.toBeInTheDocument();
  });
});
