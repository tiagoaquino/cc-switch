import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { GeminiAuthFilesSection } from "./GeminiConfigSections";

describe("GeminiAuthFilesSection", () => {
  it("shows editors only when enabled", () => {
    const onEnabledChange = vi.fn();

    const { rerender } = render(
      <GeminiAuthFilesSection
        enabled={false}
        onEnabledChange={onEnabledChange}
        googleAccountsValue=""
        oauthCredsValue=""
        onGoogleAccountsChange={() => undefined}
        onOauthCredsChange={() => undefined}
      />,
    );

    expect(screen.queryByText("google_accounts.json")).not.toBeInTheDocument();

    rerender(
      <GeminiAuthFilesSection
        enabled={true}
        onEnabledChange={onEnabledChange}
        googleAccountsValue="{}"
        oauthCredsValue="{}"
        onGoogleAccountsChange={() => undefined}
        onOauthCredsChange={() => undefined}
      />,
    );

    expect(screen.getByText("google_accounts.json")).toBeInTheDocument();
    expect(screen.getByText("oauth_creds.json")).toBeInTheDocument();
  });

  it("forwards toggle state changes", () => {
    const onEnabledChange = vi.fn();

    render(
      <GeminiAuthFilesSection
        enabled={false}
        onEnabledChange={onEnabledChange}
        googleAccountsValue=""
        oauthCredsValue=""
        onGoogleAccountsChange={() => undefined}
        onOauthCredsChange={() => undefined}
      />,
    );

    fireEvent.click(screen.getByRole("switch"));
    expect(onEnabledChange).toHaveBeenCalled();
  });
});
