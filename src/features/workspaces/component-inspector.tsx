import {
  Activity,
  Box,
  ExternalLink,
  Gauge,
  Network,
  Play,
  RotateCcw,
  Square,
  TerminalSquare,
  X,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import type { Workspace, WorkspaceComponent } from "./model";

const labels: Record<WorkspaceComponent["status"], string> = {
  running: "Running",
  stopped: "Stopped",
  external: "Running externally",
  warning: "Needs attention",
  starting: "Starting…",
  stopping: "Stopping…",
  error: "Failed",
};

export function ComponentInspector({
  workspace,
  component,
  onClose,
  onStart,
  onStop,
  onRestart,
  onOpenLogs,
  onOpenIde,
}: {
  workspace: Workspace;
  component: WorkspaceComponent;
  onClose: () => void;
  onStart: () => void;
  onStop: () => void;
  onRestart: () => void;
  onOpenLogs: () => void;
  onOpenIde: () => void;
}) {
  const logs = workspace.logs
    .filter((entry) => entry.componentId === component.id)
    .slice(0, 5);
  const active = ["running", "starting", "stopping", "warning"].includes(
    component.status,
  );
  const busy = ["starting", "stopping"].includes(component.status);
  const portValue = component.port
    ? `:${component.port}${component.portOrigin === "external" ? " · external" : ""}`
    : component.expectedPorts?.length
      ? `Expected :${component.expectedPorts.join(", :")}`
      : "Not detected";
  const processValue = component.pid
    ? `${component.portProcessName ? `${component.portProcessName} · ` : ""}PID ${component.pid}`
    : "Not running";

  return (
    <aside className="component-inspector" aria-label={`${component.name} inspector`}>
      <header className="inspector-header">
        <div className="inspector-title">
          <span className="component-icon">
            <Box />
          </span>
          <div>
            <strong>{component.name}</strong>
            <span>{component.category}</span>
          </div>
        </div>
        <button
          className="icon-action"
          aria-label="Close inspector"
          onClick={onClose}
          type="button"
        >
          <X />
        </button>
      </header>

      <div className="inspector-status">
        <span className={`status-dot status-dot--${component.status}`} />
        <strong>{labels[component.status]}</strong>
        {component.startedAt ? <span>since {component.startedAt}</span> : null}
      </div>

      <div className="inspector-actions">
        {component.status === "external" ? (
          <Button size="sm" variant="outline" disabled>
            <Activity data-icon="inline-start" />
            External process
          </Button>
        ) : active ? (
          <>
            {["running", "warning"].includes(component.status) ? (
              <Button size="sm" variant="outline" onClick={onRestart}>
                <RotateCcw data-icon="inline-start" />
                Restart
              </Button>
            ) : null}
            <Button size="sm" variant="outline" onClick={onStop} disabled={busy}>
              <Square data-icon="inline-start" />
              {component.status === "stopping" ? "Stopping" : "Stop"}
            </Button>
          </>
        ) : (
          <Button
            size="sm"
            onClick={onStart}
            disabled={
              !workspace.trusted ||
              workspace.executionAvailable === false ||
              component.runtimeReady === false ||
              component.portConflict ||
              busy
            }
            title={
              workspace.executionAvailable === false
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
            Run
          </Button>
        )}
        <Button
          size="sm"
          variant="outline"
          disabled={!workspace.editor}
          onClick={onOpenIde}
          title={!workspace.editor ? "Choose an editor from the workspace header first" : undefined}
        >
          <ExternalLink data-icon="inline-start" />
          {workspace.editor ? `Open in ${workspace.ide}` : "Open component"}
        </Button>
      </div>

      <section className="inspector-section">
        <h3>Runtime</h3>
        <div className="inspector-properties">
          <Property icon={<TerminalSquare />} label="Command" value={component.command} mono />
          <Property icon={<Gauge />} label="Stack" value={`${component.technology} · ${component.runtime}`} />
          {component.runtimeDiagnostics?.map((diagnostic) => (
            <Property
              icon={<Gauge />}
              key={diagnostic.key}
              label={diagnostic.label}
              value={runtimeDiagnosticValue(diagnostic)}
            />
          ))}
          <Property icon={<Network />} label="Port" value={portValue} />
          <Property icon={<Activity />} label="Process" value={processValue} />
          {component.cpuPercent !== undefined || component.memoryBytes !== undefined ? (
            <Property
              icon={<Gauge />}
              label="Resources"
              value={[
                component.cpuPercent !== undefined
                  ? `CPU ${component.cpuPercent < 10 ? component.cpuPercent.toFixed(1) : component.cpuPercent.toFixed(0)}%`
                  : undefined,
                component.memoryBytes !== undefined
                  ? formatMemory(component.memoryBytes)
                  : undefined,
              ]
                .filter(Boolean)
                .join(" · ")}
            />
          ) : null}
          {component.portOrigin === "external" && component.portProcessCwd ? (
            <Property
              icon={<Box />}
              label="External cwd"
              value={component.portProcessCwd}
              mono
            />
          ) : null}
        </div>
      </section>

      <section className="inspector-section">
        <div className="inspector-section__heading">
          <h3>Recent logs</h3>
          <button onClick={onOpenLogs} type="button">Open full logs</button>
        </div>

        <div className="inspector-log-list">
          {logs.length > 0 ? (
            logs.map((entry) => (
              <div className="inspector-log" key={entry.id}>
                <span>{entry.time}</span>
                <code>{entry.message}</code>
              </div>
            ))
          ) : (
            <div className="mini-empty">No logs for this component yet.</div>
          )}
        </div>
      </section>
    </aside>
  );
}

function formatMemory(bytes: number) {
  const mebibytes = bytes / 1024 / 1024;
  return mebibytes >= 1024
    ? `RAM ${(mebibytes / 1024).toFixed(1)} GB`
    : `RAM ${Math.max(1, Math.round(mebibytes))} MB`;
}

function runtimeDiagnosticValue(
  diagnostic: NonNullable<WorkspaceComponent["runtimeDiagnostics"]>[number],
) {
  if (diagnostic.status === "missing") {
    return diagnostic.requiredVersion
      ? `Not available · requires ${diagnostic.requiredVersion}`
      : "Not available to DECC";
  }
  if (diagnostic.status === "incompatible") {
    return `${diagnostic.installedVersion ?? "Detected"} · requires ${diagnostic.requiredVersion ?? "compatible version"}`;
  }

  const installed = diagnostic.installedVersion ?? "Available";
  return diagnostic.requiredVersion
    ? `${installed} · requires ${diagnostic.requiredVersion}`
    : installed;
}

function Property({
  icon,
  label,
  value,
  mono = false,
}: {
  icon: React.ReactNode;
  label: string;
  value: string;
  mono?: boolean;
}) {
  return (
    <div className="inspector-property">
      <span className="inspector-property__icon">{icon}</span>
      <span>
        <small>{label}</small>
        <strong className={mono ? "inspector-mono" : undefined}>{value}</strong>
      </span>
    </div>
  );
}
