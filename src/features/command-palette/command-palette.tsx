import { useMemo, useState } from "react";
import {
  Command,
  FolderPlus,
  Play,
  Search,
  Settings,
  Square,
  TerminalSquare,
} from "lucide-react";

export type PaletteAction = {
  id: string;
  label: string;
  detail?: string;
  keywords?: string;
  icon: React.ReactNode;
  shortcut?: string;
  run: () => void;
};

export function CommandPalette({
  open,
  actions,
  onClose,
}: {
  open: boolean;
  actions: PaletteAction[];
  onClose: () => void;
}) {
  const [query, setQuery] = useState("");

  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return actions;
    return actions.filter((action) =>
      [action.label, action.detail, action.keywords]
        .filter(Boolean)
        .some((value) => value!.toLowerCase().includes(needle)),
    );
  }, [actions, query]);

  if (!open) return null;

  return (
    <div className="palette-backdrop" onMouseDown={onClose}>
      <div
        className="command-palette"
        onMouseDown={(event) => event.stopPropagation()}
        role="dialog"
        aria-modal="true"
        aria-label="Command palette"
      >
        <div className="palette-search">
          <Search />
          <input
            autoFocus
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search commands, workspaces, and views"
            onKeyDown={(event) => {
              if (event.key === "Escape") onClose();
              if (event.key === "Enter" && filtered[0]) {
                filtered[0].run();
                setQuery("");
                onClose();
              }
            }}
          />
          <kbd>esc</kbd>
        </div>

        <div className="palette-results">
          {filtered.length > 0 ? (
            filtered.map((action, index) => (
              <button
                className={index === 0 ? "palette-item palette-item--active" : "palette-item"}
                key={action.id}
                onClick={() => {
                  action.run();
                  setQuery("");
                  onClose();
                }}
                type="button"
              >
                <span className="palette-item__icon">{action.icon}</span>
                <span className="palette-item__copy">
                  <strong>{action.label}</strong>
                  {action.detail ? <small>{action.detail}</small> : null}
                </span>
                {action.shortcut ? <kbd>{action.shortcut}</kbd> : null}
              </button>
            ))
          ) : (
            <div className="palette-empty">
              <Command />
              No matching command
            </div>
          )}
        </div>

        <div className="palette-footer">
          <span><Play /> Run</span>
          <span><Square /> Stop</span>
          <span><TerminalSquare /> Views</span>
          <span><FolderPlus /> Workspace</span>
          <span><Settings /> Preferences</span>
        </div>
      </div>
    </div>
  );
}
