// @vitest-environment happy-dom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { WorkspaceEditorLauncher } from "./workspace-editor-launcher";

const vscode = {
  label: "Visual Studio Code",
  path: "/Applications/Visual Studio Code.app",
};

const cursor = {
  label: "Cursor",
  path: "/Applications/Cursor.app",
};

afterEach(cleanup);

describe("WorkspaceEditorLauncher interactions", () => {
  it("opens the workspace through the primary half", () => {
    const onOpen = vi.fn();
    render(
      <WorkspaceEditorLauncher
        editors={[vscode, cursor]}
        defaultEditor={vscode}
        activeEditor={vscode}
        onSelect={vi.fn()}
        onChoose={vi.fn()}
        onRefresh={vi.fn()}
        onOpen={onOpen}
      />,
    );

    fireEvent.click(
      screen.getByRole("button", {
        name: "Open workspace in Visual Studio Code",
      }),
    );
    expect(onOpen).toHaveBeenCalledTimes(1);
  });

  it("selects an override and closes the editor menu", () => {
    const onSelect = vi.fn();
    const { container } = render(
      <WorkspaceEditorLauncher
        editors={[vscode, cursor]}
        defaultEditor={vscode}
        activeEditor={vscode}
        onSelect={onSelect}
        onChoose={vi.fn()}
        onRefresh={vi.fn()}
        onOpen={vi.fn()}
      />,
    );
    const details = container.querySelector<HTMLDetailsElement>(
      ".workspace-editor-menu",
    );
    expect(details).not.toBeNull();
    if (!details) return;
    details.open = true;

    fireEvent.click(screen.getByRole("button", { name: /Cursor/ }));

    expect(onSelect).toHaveBeenCalledWith(cursor);
    expect(details.open).toBe(false);
  });

  it("runs Scan and Choose another without leaving the menu open", () => {
    const onRefresh = vi.fn();
    const onChoose = vi.fn();
    const { container } = render(
      <WorkspaceEditorLauncher
        editors={[vscode]}
        defaultEditor={vscode}
        activeEditor={vscode}
        onSelect={vi.fn()}
        onChoose={onChoose}
        onRefresh={onRefresh}
        onOpen={vi.fn()}
      />,
    );
    const details = container.querySelector<HTMLDetailsElement>(
      ".workspace-editor-menu",
    );
    expect(details).not.toBeNull();
    if (!details) return;

    details.open = true;
    fireEvent.click(
      screen.getByRole("button", { name: "Scan installed editors" }),
    );
    expect(onRefresh).toHaveBeenCalledTimes(1);
    expect(details.open).toBe(false);

    details.open = true;
    fireEvent.click(
      screen.getByRole("button", { name: "Choose another editor…" }),
    );
    expect(onChoose).toHaveBeenCalledTimes(1);
    expect(details.open).toBe(false);
  });
});
