import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { EditorPicker } from "./editor-picker";

const vscode = {
  label: "Visual Studio Code",
  path: "/Applications/Visual Studio Code.app",
};

const cursor = {
  label: "Cursor",
  path: "/Applications/Cursor.app",
};

describe("EditorPicker", () => {
  it("does not repeat the inherited global editor as an explicit option", () => {
    const markup = renderToStaticMarkup(
      <EditorPicker
        editors={[vscode, cursor]}
        inheritLabel="Visual Studio Code"
        onSelect={vi.fn()}
        onChoose={vi.fn()}
      />,
    );

    expect(markup).toContain("Use default · Visual Studio Code");
    expect(markup).toContain(">Cursor</option>");
    expect(markup.match(/Visual Studio Code/g)).toHaveLength(1);
  });

  it("keeps a different workspace override while filtering the inherited product", () => {
    const markup = renderToStaticMarkup(
      <EditorPicker
        editors={[vscode, cursor]}
        value={cursor}
        inheritLabel="Visual Studio Code"
        onSelect={vi.fn()}
        onChoose={vi.fn()}
        onRefresh={vi.fn()}
      />,
    );

    expect(markup.match(/Visual Studio Code/g)).toHaveLength(1);
    expect(markup.match(/>Cursor<\/option>/g)).toHaveLength(1);
    expect(markup).toContain(">Scan</span>");
  });
});
