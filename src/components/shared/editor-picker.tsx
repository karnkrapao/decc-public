import { RefreshCw } from "lucide-react";
import {
  dedupeEditors,
  editorIdentityKey,
} from "@/features/workspaces/editor-utils";
import type { EditorSelection } from "@/features/workspaces/model";

const CHOOSE_EDITOR_VALUE = "__decc_choose_editor__";

export function EditorPicker({
  editors,
  value,
  inheritLabel,
  onSelect,
  onChoose,
  onRefresh,
}: {
  editors: EditorSelection[];
  value?: EditorSelection;
  inheritLabel?: string;
  onSelect: (editor: EditorSelection | undefined) => void;
  onChoose: () => void;
  onRefresh?: () => void;
}) {
  const inheritKey = inheritLabel
    ? editorIdentityKey({ label: inheritLabel })
    : undefined;
  const options = dedupeEditors(editors, value ? [value] : []).filter(
    (editor) => !inheritKey || editorIdentityKey(editor) !== inheritKey,
  );
  const selectedPath = value?.path ?? "";

  return (
    <div className="editor-picker">
      <select
        aria-label="Editor"
        value={selectedPath}
        onChange={(event) => {
          const path = event.target.value;
          if (path === CHOOSE_EDITOR_VALUE) {
            event.currentTarget.value = selectedPath;
            onChoose();
            return;
          }
          if (!path) {
            onSelect(undefined);
            return;
          }
          const editor = options.find((item) => item.path === path);
          if (editor) onSelect(editor);
        }}
      >
        {inheritLabel ? <option value="">Use default · {inheritLabel}</option> : null}
        {!inheritLabel && !value ? <option value="">Choose an editor</option> : null}
        {options.map((editor) => (
          <option key={editor.path} value={editor.path}>
            {editor.label}
          </option>
        ))}
        <option value={CHOOSE_EDITOR_VALUE}>Choose another editor…</option>
      </select>

      {onRefresh ? (
        <div className="editor-picker__actions">
          <button onClick={onRefresh} title="Scan installed editors again" type="button">
            <RefreshCw />
            <span>Scan</span>
          </button>
        </div>
      ) : null}
    </div>
  );
}
