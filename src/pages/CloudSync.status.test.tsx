/**
 * The Cloud Sync page must not offer sign-in when it could not find out
 * whether the user is signed in. Signing in replaces the stored tokens.
 */
import { describe, it, expect, beforeEach } from "vitest";
import { mockInvoke, resetInvokeMock } from "../test/tauriMock";
import { render, screen } from "@testing-library/react";
import CloudSync from "./CloudSync";

describe("CloudSync account status", () => {
  beforeEach(() => {
    resetInvokeMock();
  });

  it("offers sign-in when no account is stored", async () => {
    mockInvoke({
      account_status: { signed_in: false, webapp_url: "https://tryautomatic.app" },
      cloud_build_bundle: () => Promise.reject("Not signed in. Please sign in first."),
    });
    render(<CloudSync />);

    expect(await screen.findByRole("button", { name: "Sign in" })).toBeInTheDocument();
  });

  it("offers no sign-in when the keychain cannot be read", async () => {
    mockInvoke({
      account_status: () => Promise.reject("the keychain is locked"),
      cloud_build_bundle: () => Promise.reject("Could not read the sign-in token"),
    });
    render(<CloudSync />);

    expect(await screen.findByText(/the keychain is locked/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Sign in" })).not.toBeInTheDocument();
  });
});
