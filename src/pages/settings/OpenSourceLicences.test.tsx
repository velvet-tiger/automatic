import { describe, it, expect, beforeEach, vi } from "vitest";
import { invokeMock, mockInvoke, resetInvokeMock } from "../../test/tauriMock";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import OpenSourceLicences from "./OpenSourceLicences";

describe("OpenSourceLicences", () => {
  beforeEach(() => {
    resetInvokeMock();
    vi.mocked(revealItemInDir).mockClear();
  });

  it("says when the build has no licence files", async () => {
    mockInvoke({ read_third_party_licences: { folder: null, files: [] } });
    render(<OpenSourceLicences onClose={() => {}} />);
    await waitFor(() => {
      expect(screen.getByText(/This build does not include licence files/i)).toBeInTheDocument();
    });
    expect(screen.queryByText("Show files")).not.toBeInTheDocument();
  });

  it("shows one tab per file and switches between them", async () => {
    mockInvoke({
      read_third_party_licences: {
        folder: "/Applications/Automatic.app/Contents/Resources/licenses",
        files: [
          { file_name: "rust-crates.txt", text: "crate licences" },
          { file_name: "interface.txt", text: "npm licences" },
        ],
      },
    });
    render(<OpenSourceLicences onClose={() => {}} />);
    await waitFor(() => {
      expect(screen.getByText("crate licences")).toBeInTheDocument();
    });
    expect(screen.getByRole("tab", { name: "App components" })).toHaveAttribute("aria-selected", "true");

    fireEvent.click(screen.getByRole("tab", { name: "Interface" }));
    expect(screen.getByText("npm licences")).toBeInTheDocument();

    fireEvent.click(screen.getByText("Show files"));
    await waitFor(() => {
      expect(revealItemInDir).toHaveBeenCalledWith("/Applications/Automatic.app/Contents/Resources/licenses");
    });
  });

  it("shows the error when the files cannot be read", async () => {
    invokeMock.mockImplementation(() => Promise.reject("licence file 'x' is not a regular file"));
    render(<OpenSourceLicences onClose={() => {}} />);
    await waitFor(() => {
      expect(screen.getByText(/Could not read the licence files: licence file 'x'/)).toBeInTheDocument();
    });
  });

  it("moves focus into the dialog when it opens", () => {
    mockInvoke({ read_third_party_licences: { folder: null, files: [] } });
    render(<OpenSourceLicences onClose={() => {}} />);
    expect(screen.getByRole("dialog")).toHaveFocus();
  });

  it("closes on Escape", () => {
    mockInvoke({ read_third_party_licences: { folder: null, files: [] } });
    const onClose = vi.fn();
    render(<OpenSourceLicences onClose={onClose} />);
    fireEvent.keyDown(window, { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
