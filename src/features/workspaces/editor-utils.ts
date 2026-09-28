import type { EditorSelection } from "./model";

export function editorIdentityKey(editor: EditorSelection | { label: string }) {
  const label = editor.label.trim().toLocaleLowerCase();
  const compact = label.replace(/[._-]+/g, " ").replace(/\s+/g, " ").trim();

  if (
    compact === "vs code" ||
    compact === "visual studio code" ||
    compact === "code"
  ) {
    return "visual studio code";
  }
  if (compact === "vscode insiders" || compact === "visual studio code insiders") {
    return "visual studio code insiders";
  }
  if (compact === "codium") return "vscodium";
  return compact;
}

export function sameEditorIdentity(
  left?: EditorSelection,
  right?: EditorSelection,
) {
  return Boolean(
    left &&
      right &&
      editorIdentityKey(left) === editorIdentityKey(right),
  );
}

export function dedupeEditors(
  editors: EditorSelection[],
  preferred: EditorSelection[] = [],
) {
  const merged = new Map<string, EditorSelection>();
  for (const editor of preferred) {
    merged.set(editorIdentityKey(editor), editor);
  }
  for (const editor of editors) {
    const key = editorIdentityKey(editor);
    if (!merged.has(key)) merged.set(key, editor);
  }
  return [...merged.values()].sort((a, b) => a.label.localeCompare(b.label));
}
