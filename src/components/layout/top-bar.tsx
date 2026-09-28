import {
  Command,
  Monitor,
  Moon,
  PanelLeftClose,
  PanelLeftOpen,
  Search,
  Sun,
} from "lucide-react";
import type { AppSettings, Workspace } from "@/features/workspaces/model";
import { workspaceRunStatus } from "@/features/workspaces/workspace-run-status";

export function TopBar({
  workspace,
  section,
  sidebarCollapsed,
  theme,
  onToggleSidebar,
  onThemeChange,
  onSearch,
}: {
  workspace?: Workspace;
  section: string;
  sidebarCollapsed: boolean;
  theme: AppSettings["theme"];
  onToggleSidebar: () => void;
  onThemeChange: (theme: AppSettings["theme"]) => void;
  onSearch: () => void;
}) {
  const showWorkspaceName =
    workspace && workspace.name.trim().toLowerCase() !== "decc";
  const runStatus = workspace ? workspaceRunStatus(workspace) : undefined;

  return (
    <header className="topbar">
      <div className="topbar-left">
        <button
          aria-label={sidebarCollapsed ? "Expand sidebar" : "Collapse sidebar"}
          className="topbar-sidebar-toggle"
          onClick={onToggleSidebar}
          title={sidebarCollapsed ? "Expand sidebar" : "Collapse sidebar"}
          type="button"
        >
          {sidebarCollapsed ? <PanelLeftOpen /> : <PanelLeftClose />}
        </button>

        <div className="topbar-brand">
          <span className="brand-mark" aria-hidden="true">D</span>
          <strong>DECC</strong>
        </div>

        <span className="topbar-brand-divider" />

        <div className="topbar-context">
          {showWorkspaceName ? <span>{workspace.name}</span> : null}
          {runStatus?.active ? (
            <span
              className={
                runStatus.kind === "external"
                  ? "topbar-run-status topbar-run-status--external"
                  : ["starting", "stopping"].includes(runStatus.kind)
                    ? "topbar-run-status topbar-run-status--transition"
                    : "topbar-run-status"
              }
              title={runStatus.detail}
            >
              {runStatus.shortLabel}
            </span>
          ) : null}
          {workspace ? <span className="topbar-separator">/</span> : null}
          <span className="topbar-muted">{section}</span>
        </div>
      </div>

      <div className="topbar-actions">
        <button
          aria-label={`Theme: ${theme}. Click to change theme.`}
          className="topbar-theme-toggle"
          onClick={() =>
            onThemeChange(
              theme === "system" ? "light" : theme === "light" ? "dark" : "system",
            )
          }
          title={`Theme: ${theme}`}
          type="button"
        >
          {theme === "system" ? <Monitor /> : theme === "light" ? <Sun /> : <Moon />}
        </button>

        <button
          aria-label="Search or run a command"
          className="command-trigger"
          onClick={onSearch}
          type="button"
        >
          <Search />
          <span>Search or run a command</span>
          <kbd>
            <Command />
            K
          </kbd>
        </button>
      </div>
    </header>
  );
}
