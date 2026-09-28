import { useEffect, useState } from "react";
import {
  Check,
  ChevronDown,
  CircleDot,
  GitBranch,
  MoreHorizontal,
  Play,
  RefreshCw,
  ShieldAlert,
  Square,
  Trash2,
} from "lucide-react";
import { WorkspaceEditorLauncher } from "@/components/shared/workspace-editor-launcher";
import { useDismissDetails } from "@/components/shared/use-dismiss-details";
import { Button } from "@/components/ui/button";
import type {
  EditorSelection,
  NativeProcessHistory,
  Workspace,
  WorkspaceSection,
} from "./model";
import { WorkspaceOverview } from "./workspace-overview";
import { WorkspaceLogs } from "./workspace-logs";
import { WorkspaceEnvironment } from "./workspace-environment";
import { WorkspaceGit } from "./workspace-git";
import { ComponentInspector } from "./component-inspector";
import { workspaceAttentionReasons } from "./workspace-attention";

const sections: Array<{ id: WorkspaceSection; label: string }> = [
  { id: "overview", label: "Overview" },
  { id: "logs", label: "Logs" },
  { id: "environment", label: "Environment" },
  { id: "git", label: "Git" },
];

export function WorkspaceScreen({
  workspace,
  section,
  logRetentionDays,
  editors,
  defaultEditor,
  editorOverride,
  mutedSections,
  ignoredIssueIds,
  onEditorChange,
  onChooseEditor,
  onRefreshEditors,
  onSectionChange,
  onStartComponent,
  onStopComponent,
  onRestartComponent,
  onRunAll,
  onStopAll,
  onLoadProcessHistory,
  onClearLogs,
  onTrust,
  onRefreshGit,
  onLoadGitBranches,
  onSwitchGitBranch,
  onOpenIde,
  onRescan,
  onRemove,
  onToggleSectionMuted,
  onIgnoreIssue,
  onRestoreIssues,
  onNotify,
}: {
  workspace: Workspace;
  section: WorkspaceSection;
  logRetentionDays: number;
  editors: EditorSelection[];
  defaultEditor?: EditorSelection;
  editorOverride?: EditorSelection;
  mutedSections: WorkspaceSection[];
  ignoredIssueIds: string[];
  onEditorChange: (editor: EditorSelection | undefined) => void;
  onChooseEditor: () => void;
  onRefreshEditors: () => void;
  onSectionChange: (section: WorkspaceSection) => void;
  onStartComponent: (componentId: string) => void;
  onStopComponent: (componentId: string) => void;
  onRestartComponent: (componentId: string) => void;
  onRunAll: () => void;
  onStopAll: () => void;
  onLoadProcessHistory: (
    workspaceId: string,
    sessionId?: string,
  ) => Promise<NativeProcessHistory>;
  onClearLogs: () => void;
  onTrust: () => void;
  onRefreshGit: () => Promise<void>;
  onLoadGitBranches: () => Promise<string[]>;
  onSwitchGitBranch: (branch: string) => Promise<void>;
  onOpenIde: (options?: { componentId?: string; relativePath?: string }) => void;
  onRescan: () => void;
  onRemove: () => void;
  onToggleSectionMuted: (section: WorkspaceSection) => void;
  onIgnoreIssue: (issueId: string) => void;
  onRestoreIssues: () => void;
  onNotify: (message: string) => void;
}) {
  const [selectedComponentId, setSelectedComponentId] = useState<string | null>(null);
  const [branches, setBranches] = useState<string[]>([]);
  const [switchingBranch, setSwitchingBranch] = useState<string>();

  useEffect(() => {
    setSelectedComponentId(null);
  }, [workspace.id, section]);

  useEffect(() => {
    if (workspace.git.state !== "ready") {
      setBranches([]);
      return;
    }

    let cancelled = false;
    void onLoadGitBranches().then((items) => {
      if (!cancelled) setBranches(items);
    });
    return () => {
      cancelled = true;
    };
    // WorkspaceScreen is keyed by workspace ID; reload only when native Git state/branch changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [workspace.id, workspace.git.state, workspace.git.branch]);

  useDismissDetails(
    ".workspace-branch-picker[open], .workspace-editor-menu[open], .workspace-action-menu[open], .workspace-attention-details[open]",
  );

  const switchBranch = async (branch: string) => {
    if (!branch || branch === workspace.git.branch || switchingBranch) return;
    setSwitchingBranch(branch);
    try {
      await onSwitchGitBranch(branch);
      setBranches(await onLoadGitBranches());
    } catch {
      // The shared DECC error surface already receives the native Git error.
    } finally {
      setSwitchingBranch(undefined);
    }
  };

  const selectedComponent =
    workspace.components.find((component) => component.id === selectedComponentId) ??
    null;

  const controllableRunning = workspace.components.some((component) =>
    ["running", "starting", "stopping", "warning"].includes(component.status),
  );
  const hasRunnableReady = workspace.components.some(
    (component) =>
      component.includeInRunAll &&
      component.runtimeReady !== false &&
      !component.portConflict &&
      !["running", "external", "starting", "stopping"].includes(component.status),
  );
  const executionAvailable = workspace.executionAvailable !== false;
  const mutedSectionSet = new Set(mutedSections);
  const attentionReasons = workspaceAttentionReasons(
    workspace,
    mutedSections,
    ignoredIssueIds,
  );

  return (
    <div
      className={
        selectedComponent
          ? "workspace-page workspace-page--inspecting"
          : "workspace-page"
      }
    >
      <section className="workspace-heading">
        <div className="workspace-heading__copy">
          <div className="workspace-title-row">
            <h1>{workspace.name}</h1>
            {attentionReasons.length > 0 ? (
              <details className="workspace-attention-details">
                <summary className="health-pill health-pill--attention">
                  <CircleDot />
                  Needs attention
                </summary>
                <div className="workspace-attention-popover">
                  <strong>Needs attention because</strong>
                  <div className="workspace-attention-reasons">
                    {attentionReasons.slice(0, 4).map((reason) => (
                      <div className="workspace-attention-reason" key={reason.id}>
                        <span>{reason.title}</span>
                        {reason.detail ? <small>{reason.detail}</small> : null}
                      </div>
                    ))}
                  </div>
                  {attentionReasons.length > 4 ? (
                    <small className="workspace-attention-more">
                      +{attentionReasons.length - 4} more
                    </small>
                  ) : null}
                </div>
              </details>
            ) : null}
          </div>

          <div className="workspace-meta">
            <span className="workspace-path">{workspace.path}</span>
            <span className="meta-divider" />
            {workspace.git.state === "ready" && branches.length > 0 ? (
              <details
                className={
                  switchingBranch || controllableRunning
                    ? "workspace-branch-picker workspace-branch-picker--disabled"
                    : "workspace-branch-picker"
                }
              >
                <summary
                  aria-disabled={Boolean(switchingBranch) || controllableRunning}
                  aria-label="Switch Git branch"
                  onClick={(event) => {
                    if (switchingBranch || controllableRunning) event.preventDefault();
                  }}
                  title={
                    controllableRunning
                      ? "Stop DECC-owned workspace processes before switching branches"
                      : switchingBranch
                        ? `Switching to ${switchingBranch}…`
                        : "Switch Git branch"
                  }
                >
                  <GitBranch />
                  <span>
                    {switchingBranch
                      ? `Switching to ${switchingBranch}…`
                      : workspace.git.detached
                        ? "Detached HEAD"
                        : workspace.git.branch}
                  </span>
                  <ChevronDown className="workspace-branch-picker__chevron" />
                </summary>
                <div className="workspace-branch-picker__menu" role="menu">
                  <div className="workspace-branch-picker__label">Branches</div>
                  <div className="workspace-branch-picker__list">
                    {branches.map((branch) => {
                      const active = !workspace.git.detached && branch === workspace.git.branch;
                      return (
                        <button
                          aria-current={active ? "true" : undefined}
                          className={
                            active
                              ? "workspace-branch-picker__option workspace-branch-picker__option--active"
                              : "workspace-branch-picker__option"
                          }
                          key={branch}
                          onClick={(event) => {
                            event.currentTarget.closest("details")?.removeAttribute("open");
                            void switchBranch(branch);
                          }}
                          role="menuitem"
                          type="button"
                        >
                          <GitBranch />
                          <span>{branch}</span>
                          {active ? <Check /> : null}
                        </button>
                      );
                    })}
                  </div>
                </div>
              </details>
            ) : (
              <span className="workspace-git-status">
                <GitBranch />
                {workspace.branch}
              </span>
            )}
          </div>
        </div>

        <div className="workspace-heading__actions">
          <WorkspaceEditorLauncher
            editors={editors}
            defaultEditor={defaultEditor}
            value={editorOverride}
            activeEditor={workspace.editor}
            onSelect={onEditorChange}
            onChoose={onChooseEditor}
            onRefresh={onRefreshEditors}
            onOpen={() => onOpenIde()}
          />

          <details className="workspace-action-menu">
            <summary
              aria-label="Workspace actions"
              className="workspace-action-menu__trigger"
              title="More workspace actions"
            >
              <MoreHorizontal />
            </summary>
            <div className="workspace-action-menu__popover">
              <button
                disabled={controllableRunning}
                onClick={(event) => {
                  event.currentTarget.closest("details")?.removeAttribute("open");
                  onRescan();
                }}
                title={
                  controllableRunning
                    ? "Stop DECC-owned processes before rescanning"
                    : undefined
                }
                type="button"
              >
                <RefreshCw />
                Rescan workspace
              </button>
              <button
                className="workspace-action-menu__danger"
                disabled={controllableRunning}
                onClick={(event) => {
                  event.currentTarget.closest("details")?.removeAttribute("open");
                  onRemove();
                }}
                title={
                  controllableRunning
                    ? "Stop DECC-owned processes before removing"
                    : undefined
                }
                type="button"
              >
                <Trash2 />
                Remove from DECC
              </button>
            </div>
          </details>

          <span className="workspace-heading__action-divider" aria-hidden="true" />

          <Button
            aria-label="Stop all workspace components"
            variant="ghost"
            size="icon"
            onClick={onStopAll}
            disabled={!controllableRunning}
            title="Stop all"
          >
            <Square />
          </Button>
          <Button
            onClick={onRunAll}
            disabled={!workspace.trusted || !executionAvailable || !hasRunnableReady}
            title={
              !executionAvailable
                ? "Rescan this workspace to enable native process control"
                : "Run all ready components"
            }
          >
            <Play data-icon="inline-start" />
            Run all
          </Button>
        </div>
      </section>

      {!workspace.trusted && (
        <div className="trust-banner">
          <span className="trust-banner__icon">
            <ShieldAlert />
          </span>
          <div>
            <strong>Review commands before running this workspace</strong>
            <p>
              DECC found runnable commands in this folder. Trust the workspace
              before allowing DECC to execute them.
            </p>
          </div>
          <Button size="sm" onClick={onTrust}>
            Trust workspace
          </Button>
        </div>
      )}

      <nav className="workspace-tabs" aria-label="Workspace sections">
        {sections.map((item) => (
          <button
            className={
              item.id === section
                ? "workspace-tab workspace-tab--active"
                : "workspace-tab"
            }
            key={item.id}
            onClick={() => onSectionChange(item.id)}
            type="button"
          >
            {item.label}
            {item.id === "logs" &&
            !mutedSectionSet.has("logs") &&
            workspace.logs.length > 0 ? (
              <span className="tab-count">{workspace.logs.length}</span>
            ) : null}
            {item.id === "environment" &&
            !mutedSectionSet.has("environment") &&
            workspace.environmentVariables.some(
              (variable) => variable.status === "missing",
            ) ? (
              <span
                className="tab-alert"
                aria-label="Missing environment variables"
              />
            ) : null}
            {item.id === "git" &&
            !mutedSectionSet.has("git") &&
            workspace.git.changes.length > 0 ? (
              <span className="tab-count">
                {workspace.git.changes.length}
                {workspace.git.changesTruncated ? "+" : ""}
              </span>
            ) : null}
          </button>
        ))}
      </nav>

      {section === "overview" && (
        <WorkspaceOverview
          workspace={workspace}
          onStartComponent={onStartComponent}
          onStopComponent={onStopComponent}
          onRestartComponent={onRestartComponent}
          onOpenLogs={() => onSectionChange("logs")}
          onRescan={onRescan}
          onInspectComponent={setSelectedComponentId}
          mutedSections={mutedSections}
          ignoredIssueIds={ignoredIssueIds}
          onIgnoreIssue={onIgnoreIssue}
          onRestoreIssues={onRestoreIssues}
        />
      )}
      {section === "logs" && (
        <WorkspaceLogs
          workspace={workspace}
          retentionDays={logRetentionDays}
          notificationMuted={mutedSectionSet.has("logs")}
          onToggleNotification={() => onToggleSectionMuted("logs")}
          onLoadHistory={onLoadProcessHistory}
          onClear={onClearLogs}
        />
      )}
      {section === "environment" && (
        <WorkspaceEnvironment
          workspace={workspace}
          notificationMuted={mutedSectionSet.has("environment")}
          onToggleNotification={() => onToggleSectionMuted("environment")}
          onOpenEnv={() => {
            const target =
              workspace.environmentFiles.find(
                (file) => file.kind === "active" || file.kind === "override",
              ) ?? workspace.environmentFiles[0];
            if (!target) {
              onNotify("No environment file is available to open");
              return;
            }
            if (workspace.editor) {
              onOpenIde({ relativePath: target.path });
            } else {
              onChooseEditor();
            }
          }}
        />
      )}
      {section === "git" && (
        <WorkspaceGit
          workspace={workspace}
          notificationMuted={mutedSectionSet.has("git")}
          onToggleNotification={() => onToggleSectionMuted("git")}
          onRefresh={onRefreshGit}
        />
      )}

      {selectedComponent ? (
        <ComponentInspector
          workspace={workspace}
          component={selectedComponent}
          onClose={() => setSelectedComponentId(null)}
          onStart={() => onStartComponent(selectedComponent.id)}
          onStop={() => onStopComponent(selectedComponent.id)}
          onRestart={() => onRestartComponent(selectedComponent.id)}
          onOpenLogs={() => {
            setSelectedComponentId(null);
            onSectionChange("logs");
          }}
          onOpenIde={() =>
            workspace.editor
              ? onOpenIde({ componentId: selectedComponent.id })
              : onChooseEditor()
          }
        />
      ) : null}
    </div>
  );
}
