import {
  Activity,
  AlertTriangle,
  Box,
  HardDrive,
  Play,
  RotateCcw,
  Square,
  TerminalSquare,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import type {
  ComponentStatus,
  Workspace,
  WorkspaceComponent,
  WorkspaceSection,
} from "./model";

const statusMeta: Record<
  ComponentStatus,
  { label: string; className: string }
> = {
  running: { label: "Running", className: "status-dot status-dot--running" },
  stopped: { label: "Stopped", className: "status-dot status-dot--stopped" },
  external: { label: "External", className: "status-dot status-dot--external" },
  warning: { label: "Attention", className: "status-dot status-dot--warning" },
  starting: { label: "Starting…", className: "status-dot status-dot--starting" },
  stopping: { label: "Stopping…", className: "status-dot status-dot--stopping" },
  error: { label: "Failed", className: "status-dot status-dot--error" },
};

export function WorkspaceOverview({
  workspace,
  onStartComponent,
  onStopComponent,
  onRestartComponent,
  onOpenLogs,
  onRescan,
  onInspectComponent,
  mutedSections,
  ignoredIssueIds,
  onIgnoreIssue,
  onRestoreIssues,
}: {
  workspace: Workspace;
  onStartComponent: (componentId: string) => void;
  onStopComponent: (componentId: string) => void;
  onRestartComponent: (componentId: string) => void;
  onOpenLogs: () => void;
  onRescan: () => void;
  onInspectComponent: (componentId: string) => void;
  mutedSections: WorkspaceSection[];
  ignoredIssueIds: string[];
  onIgnoreIssue: (issueId: string) => void;
  onRestoreIssues: () => void;
}) {
  const runningCount = workspace.components.filter((component) =>
    ["running", "starting"].includes(component.status),
  ).length;
  const externalCount = workspace.components.filter(
    (component) => component.status === "external",
  ).length;
  const cpuTotal = workspace.components.reduce(
    (sum, component) => sum + (component.cpuPercent ?? 0),
    0,
  );
  const memoryTotal = workspace.components.reduce(
    (sum, component) => sum + (component.memoryBytes ?? 0),
    0,
  );
  const hasResourceUsage = workspace.components.some(
    (component) =>
      component.cpuPercent !== undefined || component.memoryBytes !== undefined,
  );
  const ignoredIssueSet = new Set(ignoredIssueIds);
  const mutedSectionSet = new Set(mutedSections);
  const visibleIssues = workspace.issues.filter(
    (issue) =>
      !ignoredIssueSet.has(issue.id) &&
      !(mutedSectionSet.has("environment") && issue.id.startsWith("environment:")),
  );
  const ignoredIssueCount = workspace.issues.filter((issue) =>
    ignoredIssueSet.has(issue.id),
  ).length;

  if (workspace.components.length === 0) {
    return (
      <section className="workspace-section-page">
        <div className="system-empty">
          <span className="system-empty__icon">
            <Box />
          </span>
          <strong>No runnable components detected</strong>
          <p>
            This workspace is valid, but DECC did not find an app, service,
            worker, or known run command yet.
          </p>
          <div className="system-empty__actions">
            <Button size="sm" onClick={onRescan}>
              Scan again
            </Button>
          </div>
        </div>
      </section>
    );
  }

  return (
    <>
      <div className="workspace-summary">
        <SummaryItem
          icon={<Activity />}
          label="Processes"
          value={runningCount > 0 ? `${runningCount} running` : "Idle"}
          detail={
            hasResourceUsage
              ? [formatCpu(cpuTotal), formatMemory(memoryTotal)]
                  .filter(Boolean)
                  .join(" · ")
              : externalCount > 0
                ? `${externalCount} external`
                : `${workspace.components.length} components`
          }
        />
        <SummaryItem
          icon={<TerminalSquare />}
          label="Run plan"
          value={`${workspace.components.filter((component) => component.includeInRunAll).length} included`}
          detail="Parallel where order is unknown"
        />
        <SummaryItem
          icon={<HardDrive />}
          label="Workspace"
          value={workspace.trusted ? "Trusted" : "Review required"}
          detail="Configuration stored locally"
        />
      </div>

      {(visibleIssues.length > 0 || ignoredIssueCount > 0) && (
        <section className="notice-list" aria-label="Workspace issues">
          {visibleIssues.map((issue) => {
            const action =
              issue.action ?? (issue.componentId ? "inspect" : "logs");
            return (
              <div
                className={
                  issue.tone === "danger"
                    ? "notice notice--danger"
                    : "notice"
                }
                key={issue.id}
              >
                <span className="notice-icon">
                  <AlertTriangle />
                </span>
                <div>
                  <strong>{issue.title}</strong>
                  <p>{issue.detail}</p>
                </div>
                <div className="notice-actions">
                  <button
                    onClick={() => {
                      if (action === "rescan") {
                        onRescan();
                      } else if (action === "inspect" && issue.componentId) {
                        onInspectComponent(issue.componentId);
                      } else {
                        onOpenLogs();
                      }
                    }}
                    type="button"
                  >
                    {action === "rescan"
                      ? "Rescan & choose"
                      : action === "inspect"
                        ? "Inspect component"
                        : "Inspect process"}
                  </button>
                  <button onClick={() => onIgnoreIssue(issue.id)} type="button">
                    Ignore
                  </button>
                </div>
              </div>
            );
          })}
          {ignoredIssueCount > 0 ? (
            <button
              className="notice-restore"
              onClick={onRestoreIssues}
              type="button"
            >
              Restore {ignoredIssueCount} ignored notice
              {ignoredIssueCount === 1 ? "" : "s"}
            </button>
          ) : null}
        </section>
      )}

      <section className="content-section">
        <div className="section-heading">
          <div>
            <h2>Components</h2>
            <p>Run and inspect the parts that make up this workspace.</p>
          </div>

        </div>

        <div className="component-table">
          <div className="component-table__head" aria-hidden="true">
            <span>Component</span>
            <span>Command</span>
            <span>Status</span>
            <span />
          </div>

          <div className="component-list">
            {workspace.components.map((component) => (
              <ComponentRow
                component={component}
                key={component.id}
                trusted={workspace.trusted}
                executionAvailable={workspace.executionAvailable !== false}
                onStart={() => onStartComponent(component.id)}
                onStop={() => onStopComponent(component.id)}
                onRestart={() => onRestartComponent(component.id)}
                onInspect={() => onInspectComponent(component.id)}
              />
            ))}
          </div>
        </div>
      </section>

      <section className="content-section content-section--activity">
        <div className="section-heading">
          <div>
            <h2>Recent activity</h2>
            <p>Latest output and lifecycle events.</p>
          </div>
          <button className="quiet-action" onClick={onOpenLogs} type="button">
            Open logs
          </button>
        </div>

        <div className="activity-list">
          {workspace.activity.length > 0 ? (
            workspace.activity.slice(0, 7).map((item) => (
              <div className="activity-row" key={item.id}>
                <span
                  className={
                    item.level === "success"
                      ? "activity-level activity-level--success"
                      : item.level === "error"
                        ? "activity-level activity-level--error"
                        : item.level === "warning"
                          ? "activity-level activity-level--warning"
                          : "activity-level"
                  }
                />
                <span className="activity-time">{item.time}</span>
                <span className="activity-source">{item.source}</span>
                <span
                  className={
                    item.level === "error"
                      ? "activity-message activity-message--error"
                      : "activity-message"
                  }
                >
                  {item.message}
                </span>
              </div>
            ))
          ) : (
            <div className="mini-empty">No activity recorded yet.</div>
          )}
        </div>
      </section>
    </>
  );
}

function formatCpu(value: number) {
  return `CPU ${value < 10 ? value.toFixed(1) : value.toFixed(0)}%`;
}

function formatMemory(bytes: number) {
  const mebibytes = bytes / 1024 / 1024;
  return mebibytes >= 1024
    ? `RAM ${(mebibytes / 1024).toFixed(1)} GB`
    : `RAM ${Math.max(1, Math.round(mebibytes))} MB`;
}

function SummaryItem({
  icon,
  label,
  value,
  detail,
}: {
  icon: React.ReactNode;
  label: string;
  value: string;
  detail: string;
}) {
  return (
    <div className="summary-item">
      <span className="summary-icon">{icon}</span>
      <span className="summary-copy">
        <span className="summary-label">{label}</span>
        <span className="summary-value">{value}</span>
        <span className="summary-detail">{detail}</span>
      </span>
    </div>
  );
}

function ComponentRow({
  component,
  trusted,
  executionAvailable,
  onStart,
  onStop,
  onRestart,
  onInspect,
}: {
  component: WorkspaceComponent;
  trusted: boolean;
  executionAvailable: boolean;
  onStart: () => void;
  onStop: () => void;
  onRestart: () => void;
  onInspect: () => void;
}) {
  const meta = statusMeta[component.status];
  const busy = ["starting", "stopping"].includes(component.status);

  return (
    <article className="component-row" onClick={onInspect}>
      <div className="component-identity">
        <div className="component-icon" aria-hidden="true">
          <Box />
        </div>
        <div className="component-name">
          <div className="component-title">
            <strong>{component.name}</strong>
            <span className="component-category">{component.category}</span>
            {!component.includeInRunAll ? (
              <span className="component-runall-badge">Manual</span>
            ) : null}
            {component.runtimeReady === false ? (
              <span className="component-runall-badge">Tooling issue</span>
            ) : null}
          </div>
          <p>
            {component.technology}
            <span>·</span>
            {component.runtime}
            <span>·</span>
            {component.workingDirectory}
          </p>
        </div>
      </div>

      <div className="component-command">
        <TerminalSquare />
        <code>{component.command}</code>
      </div>

      <div className="component-runtime">
        <div className="component-status">
          <span className={meta.className} />
          <span>{meta.label}</span>
        </div>
        <div className="component-details">
          {component.port ? <span>:{component.port}</span> : null}
          {component.pid ? <span>PID {component.pid}</span> : null}
          {component.cpuPercent !== undefined ? (
            <span>{formatCpu(component.cpuPercent)}</span>
          ) : null}
          {component.memoryBytes !== undefined ? (
            <span>{formatMemory(component.memoryBytes)}</span>
          ) : null}
          {component.status === "stopped" && component.lastExitCode !== undefined ? (
            <span>exit {component.lastExitCode}</span>
          ) : null}
        </div>
      </div>

      <div className="component-actions">
        {component.status === "external" ? (
          <Button
            size="sm"
            variant="outline"
            onClick={(event) => {
              event.stopPropagation();
              onInspect();
            }}
          >
            Inspect
          </Button>
        ) : ["running", "starting", "stopping", "warning"].includes(component.status) ? (
          <>
            {["running", "warning"].includes(component.status) ? (
              <button
                className="icon-action"
                aria-label={`Restart ${component.name}`}
                onClick={(event) => {
                  event.stopPropagation();
                  onRestart();
                }}
                title="Restart component"
                type="button"
              >
                <RotateCcw />
              </button>
            ) : null}
            <Button
              size="sm"
              variant="outline"
              onClick={(event) => {
                event.stopPropagation();
                onStop();
              }}
              disabled={busy}
            >
              <Square data-icon="inline-start" />
              {component.status === "starting"
                ? "Starting"
                : component.status === "stopping"
                  ? "Stopping"
                  : "Stop"}
            </Button>
          </>
        ) : (
          <Button
            size="sm"
            variant="outline"
            onClick={(event) => {
              event.stopPropagation();
              onStart();
            }}
            disabled={
              !trusted ||
              !executionAvailable ||
              component.runtimeReady === false ||
              component.portConflict ||
              busy
            }
            title={
              !executionAvailable
                ? "Rescan this workspace to enable native process control"
                : component.runtimeReady === false
                  ? component.runtimeBlockingReason ??
                    "Resolve the runtime/tooling issue before starting this component"
                  : component.portConflict
                    ? "Resolve the port conflict before starting this component"
                    : undefined
            }
          >
            <Play data-icon="inline-start" />
            {component.status === "starting" ? "Starting" : "Run"}
          </Button>
        )}

      </div>
    </article>
  );
}
