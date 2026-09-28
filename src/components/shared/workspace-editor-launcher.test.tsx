import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { WorkspaceEditorLauncher } from "./workspace-editor-launcher";

const vscode = {
  label: "Visual Studio Code",
  path: "/Applications/Visual Studio Code.app",
};

const cursor = {
  label: "Cursor",
  path: "/Applications/Cursor.app",
};

describe("WorkspaceEditorLauncher", () => {
  it("combines open and editor management into one compact launcher", () => {
    const markup = renderToStaticMarkup(
      <WorkspaceEditorLauncher
        editors={[vscode, cursor]}
        defaultEditor={vscode}
        activeEditor={vscode}
        onSelect={vi.fn()}
        onChoose={vi.fn()}
        onRefresh={vi.fn()}
        onOpen={vi.fn()}
      />,
    );

    expect(markup).toContain("Open in Visual Studio Code");
    expect(markup).toContain("Use default");
    expect(markup).toContain("Scan installed editors");
    expect(markup).toContain("Choose another editor…");
    expect(markup).toContain(">Visual Studio Code</small>");
    expect(markup).not.toContain("<strong>Visual Studio Code</strong>");
  });

  it("shows a workspace override once and keeps the inherited editor out of explicit choices", () => {
    const markup = renderToStaticMarkup(
      <WorkspaceEditorLauncher
        editors={[vscode, cursor, { ...cursor, path: "/usr/local/bin/cursor" }]}
        defaultEditor={vscode}
        value={cursor}
        activeEditor={cursor}
        onSelect={vi.fn()}
        onChoose={vi.fn()}
        onRefresh={vi.fn()}
        onOpen={vi.fn()}
      />,
    );

    expect(markup).toContain("Open in Cursor");
    expect(markup.match(/<strong>Cursor<\/strong>/g)).toHaveLength(1);
    expect(markup).not.toContain("<strong>Visual Studio Code</strong>");
  });
});
