import {
  Check,
  ChevronDown,
  ExternalLink,
  FolderOpen,
  RefreshCw,
} from "lucide-react";
import {
  dedupeEditors,
  editorIdentityKey,
} from "@/features/workspaces/editor-utils";
import type { EditorSelection } from "@/features/workspaces/model";

function closeMenu(target: HTMLElement) {
  target.closest("details")?.removeAttribute("open");
}

export function WorkspaceEditorLauncher({
  editors,
  defaultEditor,
  value,
  activeEditor,
  onSelect,
  onChoose,
  onRefresh,
  onOpen,
}: {
  editors: EditorSelection[];
  defaultEditor?: EditorSelection;
  value?: EditorSelection;
  activeEditor?: EditorSelection;
  onSelect: (editor: EditorSelection | undefined) => void;
  onChoose: () => void;
  onRefresh: () => void;
  onOpen: () => void;
}) {
  const inheritedKey = defaultEditor
    ? editorIdentityKey(defaultEditor)
    : undefined;
  const options = dedupeEditors(editors, value ? [value] : []).filter(
    (editor) =>
      !inheritedKey || editorIdentityKey(editor) !== inheritedKey,
  );
  const activeOverrideKey = value ? editorIdentityKey(value) : undefined;
  const activeLabel = activeEditor?.label ?? "Choose editor";

  return (
    <div className="workspace-editor-launcher">
      <button
        aria-label={
          activeEditor
            ? `Open workspace in ${activeLabel}`
            : "Choose an editor before opening the workspace"
        }
        className="workspace-editor-launcher__open"
        disabled={!activeEditor}
        onClick={onOpen}
        title={
          activeEditor
            ? `Open workspace in ${activeLabel}`
            : "Choose an editor first"
        }
        type="button"
      >
        <ExternalLink />
        <span>{activeEditor ? `Open in ${activeLabel}` : "Open editor"}</span>
      </button>

      <details className="workspace-editor-menu">
        <summary
          aria-label="Choose editor"
          title="Editor options"
        >
          <ChevronDown />
        </summary>
        <div className="workspace-editor-menu__popover">
          <div className="workspace-editor-menu__label">
            Editor for this workspace
          </div>

          <div className="workspace-editor-menu__options">
            {defaultEditor ? (
              <button
                className={!value ? "workspace-editor-menu__option workspace-editor-menu__option--active" : "workspace-editor-menu__option"}
                onClick={(event) => {
                  closeMenu(event.currentTarget);
                  onSelect(undefined);
                }}
                type="button"
              >
                <span className="workspace-editor-menu__check">
                  {!value ? <Check /> : null}
                </span>
                <span className="workspace-editor-menu__option-copy">
                  <strong>Use default</strong>
                  <small>{defaultEditor.label}</small>
                </span>
              </button>
            ) : null}

            {options.map((editor) => {
              const active =
                activeOverrideKey === editorIdentityKey(editor);
              return (
                <button
                  className={
                    active
                      ? "workspace-editor-menu__option workspace-editor-menu__option--active"
                      : "workspace-editor-menu__option"
                  }
                  key={editor.path}
                  onClick={(event) => {
                    closeMenu(event.currentTarget);
                    onSelect(editor);
                  }}
                  type="button"
                >
                  <span className="workspace-editor-menu__check">
                    {active ? <Check /> : null}
                  </span>
                  <span className="workspace-editor-menu__option-copy">
                    <strong>{editor.label}</strong>
                    <small>{editor.path}</small>
                  </span>
                </button>
              );
            })}
          </div>

          <div className="workspace-editor-menu__tools">
            <button
              onClick={(event) => {
                closeMenu(event.currentTarget);
                onRefresh();
              }}
              type="button"
            >
              <RefreshCw />
              Scan installed editors
            </button>
            <button
              onClick={(event) => {
                closeMenu(event.currentTarget);
                onChoose();
              }}
              type="button"
            >
              <FolderOpen />
              Choose another editor…
            </button>
          </div>
        </div>
      </details>
    </div>
  );
}
