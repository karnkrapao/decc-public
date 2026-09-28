import {
  AlertTriangle,
  Check,
  ListFilter,
  Pin,
  Plus,
  Search,
  Settings,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import type { Workspace, WorkspaceSection } from "@/features/workspaces/model";
import { workspaceNeedsAttention } from "@/features/workspaces/workspace-attention";
import { workspaceRunStatus } from "@/features/workspaces/workspace-run-status";

type WorkspaceFilter = "all" | "running" | "attention" | "pinned";
type WorkspaceSort = "recent" | "name";

const filterOptions: Array<{
  id: WorkspaceFilter;
  label: string;
}> = [
  { id: "all", label: "All" },
  { id: "running", label: "Running" },
  { id: "attention", label: "Needs attention" },
  { id: "pinned", label: "Pinned" },
];

const sortOptions: Array<{
  id: WorkspaceSort;
  label: string;
}> = [
  { id: "recent", label: "Recent activity" },
  { id: "name", label: "Name" },
];

function initials(name: string) {
  return name
    .split(/\s+/)
    .slice(0, 2)
    .map((part) => part[0])
    .join("")
    .toUpperCase();
}

function isWorkspaceRunning(workspace: Workspace) {
  return workspaceRunStatus(workspace).active;
}

export function AppSidebar({
  workspaces,
  activeWorkspaceId,
  pinnedWorkspaceIds,
  recentWorkspaceIds,
  mutedWorkspaceSections,
  ignoredWorkspaceIssues,
  settingsActive,
  collapsed,
  onSelect,
  onTogglePin,
  onExpand,
  onAdd,
  onSettings,
}: {
  workspaces: Workspace[];
  activeWorkspaceId: string | null;
  pinnedWorkspaceIds: string[];
  recentWorkspaceIds: string[];
  mutedWorkspaceSections: Record<string, WorkspaceSection[]>;
  ignoredWorkspaceIssues: Record<string, string[]>;
  settingsActive: boolean;
  collapsed: boolean;
  onSelect: (id: string) => void;
  onTogglePin: (id: string) => void;
  onExpand: () => void;
  onAdd: () => void;
  onSettings: () => void;
}) {
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<WorkspaceFilter>("all");
  const [sort, setSort] = useState<WorkspaceSort>("name");
  const [filterOpen, setFilterOpen] = useState(false);
  const toolsRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!filterOpen) return;

    function onPointerDown(event: MouseEvent) {
      if (!toolsRef.current?.contains(event.target as Node)) {
        setFilterOpen(false);
      }
    }

    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") setFilterOpen(false);
    }

    window.addEventListener("mousedown", onPointerDown);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("mousedown", onPointerDown);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [filterOpen]);

  useEffect(() => {
    if (collapsed) setFilterOpen(false);
  }, [collapsed]);

  const visibleWorkspaces = useMemo(() => {
    const recentRank = new Map(
      recentWorkspaceIds.map((id, index) => [id, index]),
    );
    const needle = query.trim().toLowerCase();

    const filtered = workspaces.filter((workspace) => {
      if (
        needle &&
        ![workspace.name, workspace.path, workspace.branch]
          .join(" ")
          .toLowerCase()
          .includes(needle)
      ) {
        return false;
      }

      if (collapsed) return true;
      if (filter === "running") return isWorkspaceRunning(workspace);
      if (filter === "attention") {
        return workspaceNeedsAttention(
          workspace,
          mutedWorkspaceSections[workspace.id] ?? [],
          ignoredWorkspaceIssues[workspace.id] ?? [],
        );
      }
      if (filter === "pinned") return pinnedWorkspaceIds.includes(workspace.id);
      return true;
    });

    return [...filtered].sort((a, b) => {
      const aPinned = pinnedWorkspaceIds.includes(a.id);
      const bPinned = pinnedWorkspaceIds.includes(b.id);

      if (aPinned !== bPinned) return aPinned ? -1 : 1;

      if (sort === "name") {
        return a.name.localeCompare(b.name);
      }

      return (
        (recentRank.get(a.id) ?? Number.MAX_SAFE_INTEGER) -
        (recentRank.get(b.id) ?? Number.MAX_SAFE_INTEGER)
      );
    });
  }, [
    collapsed,
    pinnedWorkspaceIds,
    filter,
    ignoredWorkspaceIssues,
    mutedWorkspaceSections,
    query,
    recentWorkspaceIds,
    sort,
    workspaces,
  ]);

  const activeFilterCount = Number(filter !== "all") + Number(sort !== "name");

  return (
    <aside className="sidebar">
      {collapsed ? (
        <button
          aria-label="Expand sidebar to search workspaces"
          className="sidebar-search sidebar-search--collapsed"
          onClick={onExpand}
          title="Search workspaces"
          type="button"
        >
          <Search />
        </button>
      ) : (
        <div className="sidebar-tools" ref={toolsRef}>
          <label className="sidebar-search">
            <Search />
            <input
              aria-label="Search workspaces"
              placeholder="Search workspaces"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
            />
          </label>

          <button
            aria-expanded={filterOpen}
            aria-haspopup="menu"
            aria-label="Filter and sort workspaces"
            className={
              filterOpen || activeFilterCount > 0
                ? "sidebar-filter-trigger sidebar-filter-trigger--active"
                : "sidebar-filter-trigger"
            }
            onClick={() => setFilterOpen((open) => !open)}
            title="Filter and sort"
            type="button"
          >
            <ListFilter />
            {activeFilterCount > 0 ? (
              <span className="sidebar-filter-count">{activeFilterCount}</span>
            ) : null}
          </button>

          {filterOpen ? (
            <div className="sidebar-filter-popover" role="menu">
              <div className="sidebar-filter-group">
                <span className="sidebar-filter-label">Show</span>
                {filterOptions.map((option) => (
                  <button
                    className="sidebar-filter-option"
                    key={option.id}
                    onClick={() => setFilter(option.id)}
                    role="menuitemradio"
                    aria-checked={filter === option.id}
                    type="button"
                  >
                    <span>{option.label}</span>
                    {filter === option.id ? <Check /> : null}
                  </button>
                ))}
              </div>

              <div className="sidebar-filter-divider" />

              <div className="sidebar-filter-group">
                <span className="sidebar-filter-label">Sort by</span>
                {sortOptions.map((option) => (
                  <button
                    className="sidebar-filter-option"
                    key={option.id}
                    onClick={() => setSort(option.id)}
                    role="menuitemradio"
                    aria-checked={sort === option.id}
                    type="button"
                  >
                    <span>{option.label}</span>
                    {sort === option.id ? <Check /> : null}
                  </button>
                ))}
              </div>

              {activeFilterCount > 0 ? (
                <>
                  <div className="sidebar-filter-divider" />
                  <button
                    className="sidebar-filter-reset"
                    onClick={() => {
                      setFilter("all");
                      setSort("name");
                    }}
                    type="button"
                  >
                    Reset view
                  </button>
                </>
              ) : null}
            </div>
          ) : null}
        </div>
      )}

      <div className="sidebar-scroll">
        <div className="workspace-list">
          {visibleWorkspaces.map((workspace) => (
            <WorkspaceRow
              key={workspace.id}
              workspace={workspace}
              active={workspace.id === activeWorkspaceId && !settingsActive}
              pinned={pinnedWorkspaceIds.includes(workspace.id)}
              attention={workspaceNeedsAttention(
                workspace,
                mutedWorkspaceSections[workspace.id] ?? [],
                ignoredWorkspaceIssues[workspace.id] ?? [],
              )}
              collapsed={collapsed}
              onSelect={() => onSelect(workspace.id)}
              onTogglePin={() => onTogglePin(workspace.id)}
            />
          ))}
        </div>

        {!collapsed && visibleWorkspaces.length === 0 ? (
          <div className="sidebar-empty">
            {query.trim()
              ? "No matching workspace"
              : filter === "running"
                ? "No running workspaces"
                : filter === "attention"
                  ? "No workspaces need attention"
                  : filter === "pinned"
                    ? "No pinned workspaces"
                    : "No workspaces"}
          </div>
        ) : null}

      </div>

      <div className="sidebar-footer">
        <button
          aria-label="Add workspace"
          className="sidebar-footer__item sidebar-footer__item--add"
          onClick={onAdd}
          title="Add workspace"
          type="button"
        >
          <Plus />
          <span>Add workspace</span>
        </button>
        <button
          aria-current={settingsActive ? "page" : undefined}
          aria-label="Settings"
          className={
            settingsActive
              ? "sidebar-footer__item sidebar-footer__item--active"
              : "sidebar-footer__item"
          }
          onClick={onSettings}
          title="Settings"
          type="button"
        >
          <Settings />
          <span>Settings</span>
        </button>
      </div>
    </aside>
  );
}

function WorkspaceRow({
  workspace,
  active,
  pinned,
  attention,
  collapsed,
  onSelect,
  onTogglePin,
}: {
  workspace: Workspace;
  active: boolean;
  pinned: boolean;
  attention: boolean;
  collapsed: boolean;
  onSelect: () => void;
  onTogglePin: () => void;
}) {
  const runStatus = workspaceRunStatus(workspace);
  const runTone =
    runStatus.kind === "external"
      ? "external"
      : ["starting", "stopping"].includes(runStatus.kind)
        ? "transition"
        : "running";

  return (
    <div className="workspace-nav-row">
      <button
        aria-current={active ? "page" : undefined}
        aria-label={
          runStatus.active
            ? `${workspace.name}, ${runStatus.detail}`
            : workspace.name
        }
        className={active ? "workspace-nav workspace-nav--active" : "workspace-nav"}
        onClick={onSelect}
        title={
          collapsed
            ? runStatus.active
              ? `${workspace.name} · ${runStatus.shortLabel}`
              : workspace.name
            : undefined
        }
        type="button"
      >
        <span className="workspace-avatar" aria-hidden="true">
          {initials(workspace.name)}
          {collapsed && attention ? (
            <span className="workspace-health workspace-health--attention" />
          ) : null}
          {collapsed && runStatus.active ? (
            <span
              className={`workspace-run-rail workspace-run-rail--${runTone} ${runStatus.total > 1 ? "workspace-run-rail--ratio" : "workspace-run-rail--dot"}`}
              title={runStatus.detail}
            >
              {runStatus.total > 1 ? (
                <span>{runStatus.activeCount}/{runStatus.total}</span>
              ) : (
                <span className="workspace-run-rail__dot" />
              )}
            </span>
          ) : null}
        </span>

        <span className="workspace-nav__copy">
          <span className="workspace-nav__name">
            {workspace.name}
            {pinned ? <Pin className="workspace-pin-indicator" /> : null}
          </span>
          <span className="workspace-nav__branch">
            {workspace.components.length === 0 ? "No components" : workspace.branch}
          </span>
        </span>

        <span className="workspace-nav__states" aria-hidden="true">
          {runStatus.active ? (
            <span
              className={`workspace-status-mini workspace-status-mini--${runTone}`}
              title={runStatus.detail}
            >
              <span className="workspace-status-mini__dot" />
              {runStatus.total > 1 ? (
                <span className="workspace-status-mini__ratio">
                  {runStatus.activeCount}/{runStatus.total}
                </span>
              ) : null}
            </span>
          ) : null}
          {attention ? (
            <span className="workspace-attention-dot">
              <AlertTriangle />
            </span>
          ) : null}
        </span>
      </button>

      <button
        aria-label={pinned ? "Unpin workspace" : "Pin workspace"}
        className={
          pinned
            ? "workspace-pin workspace-pin--active"
            : "workspace-pin"
        }
        onClick={onTogglePin}
        title={pinned ? "Unpin workspace" : "Pin workspace"}
        type="button"
      >
        <Pin />
      </button>
    </div>
  );
}
