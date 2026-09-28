import { useEffect, useMemo, useState } from "react";
import {
  Bell,
  BellOff,
  Circle,
  Eraser,
  Filter,
  History,
  Pause,
  Play,
  Search,
  TerminalSquare,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import type {
  NativeProcessEvent,
  NativeProcessHistory,
  NativeRunSessionMetadata,
  Workspace,
  WorkspaceLog,
} from "./model";

function eventLog(event: NativeProcessEvent, index: number): WorkspaceLog {
  const time = new Date(event.timestampMs).toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  });

  return {
    id: [
      "history",
      event.timestampMs,
      event.componentId,
      event.kind,
      event.stream ?? "system",
      event.message,
      index,
    ].join("-"),
    time,
    componentId: event.componentId,
    component: event.componentName,
    level:
      event.kind === "error"
        ? "error"
        : event.stream === "stderr"
          ? "warning"
          : event.kind === "started"
            ? "success"
            : "info",
    stream: event.stream ?? "system",
    message: event.message,
  };
}

function sessionLabel(session: NativeRunSessionMetadata) {
  const started = new Date(session.startedAtMs).toLocaleString([], {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
  const status =
    session.status === "running" || session.status === "starting"
      ? "active"
      : session.exitCode === undefined
        ? session.status
        : `exit ${session.exitCode}`;
  return `${session.componentName} · ${started} · ${status}`;
}

export function WorkspaceLogs({
  workspace,
  retentionDays,
  notificationMuted,
  onToggleNotification,
  onLoadHistory,
  onClear,
}: {
  workspace: Workspace;
  retentionDays: number;
  notificationMuted: boolean;
  onToggleNotification: () => void;
  onLoadHistory: (
    workspaceId: string,
    sessionId?: string,
  ) => Promise<NativeProcessHistory>;
  onClear: () => void;
}) {
  const [componentId, setComponentId] = useState("all");
  const [query, setQuery] = useState("");
  const [follow, setFollow] = useState(true);
  const [sessions, setSessions] = useState<NativeRunSessionMetadata[]>([]);
  const [selectedSessionId, setSelectedSessionId] = useState("recent");
  const [sessionLogs, setSessionLogs] = useState<WorkspaceLog[]>([]);
  const [historyLoading, setHistoryLoading] = useState(false);
  const [historyError, setHistoryError] = useState<string>();

  useEffect(() => {
    let cancelled = false;
    setSelectedSessionId("recent");
    setSessionLogs([]);
    setHistoryError(undefined);

    void onLoadHistory(workspace.id)
      .then((history) => {
        if (!cancelled) setSessions(history.sessions);
      })
      .catch((error) => {
        if (!cancelled) {
          setHistoryError(error instanceof Error ? error.message : String(error));
        }
      });

    return () => {
      cancelled = true;
    };
  }, [onLoadHistory, workspace.id]);

  const selectSession = async (sessionId: string) => {
    setSelectedSessionId(sessionId);
    setHistoryError(undefined);
    if (sessionId === "recent") {
      setSessionLogs([]);
      return;
    }

    setHistoryLoading(true);
    try {
      const history = await onLoadHistory(workspace.id, sessionId);
      setSessions(history.sessions);
      if (!history.sessions.some((session) => session.id === sessionId)) {
        setSelectedSessionId("recent");
        setSessionLogs([]);
        setHistoryError("That run session is no longer available.");
        return;
      }
      setSessionLogs(history.events.map(eventLog));
    } catch (error) {
      setSessionLogs([]);
      setHistoryError(error instanceof Error ? error.message : String(error));
    } finally {
      setHistoryLoading(false);
    }
  };

  const sourceLogs =
    selectedSessionId === "recent" ? workspace.logs : sessionLogs;

  const logs = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return sourceLogs.filter((entry) => {
      const matchesComponent =
        componentId === "all" || entry.componentId === componentId;
      const matchesQuery =
        !needle ||
        entry.message.toLowerCase().includes(needle) ||
        entry.component.toLowerCase().includes(needle);
      return matchesComponent && matchesQuery;
    });
  }, [componentId, query, sourceLogs]);

  return (
    <section className="workspace-section-page">
      <div className="section-page-heading">
        <div>
          <h2>Logs</h2>
          <p>
            Combined output from DECC-owned and observed workspace processes.
          </p>
        </div>
        <div className="section-page-actions">
          <Button
            size="sm"
            variant="outline"
            onClick={onToggleNotification}
            title={
              notificationMuted
                ? "Show the Logs tab badge again"
                : "Hide the Logs tab badge for this workspace"
            }
          >
            {notificationMuted ? (
              <Bell data-icon="inline-start" />
            ) : (
              <BellOff data-icon="inline-start" />
            )}
            {notificationMuted ? "Unmute badge" : "Mute badge"}
          </Button>
          <Button
            size="sm"
            variant={follow ? "secondary" : "outline"}
            onClick={() => setFollow((value) => !value)}
          >
            {follow ? <Pause data-icon="inline-start" /> : <Play data-icon="inline-start" />}
            {follow ? "Following" : "Follow"}
          </Button>
          <Button
            size="sm"
            variant="outline"
            onClick={() => {
              onClear();
              setSessions([]);
              setSessionLogs([]);
              setSelectedSessionId("recent");
            }}
            disabled={workspace.logs.length === 0 && sessions.length === 0}
          >
            <Eraser data-icon="inline-start" />
            Clear
          </Button>
        </div>
      </div>

      <div className="log-toolbar">
        <label className="toolbar-select toolbar-select--history">
          <History />
          <select
            aria-label="Run session"
            value={selectedSessionId}
            disabled={historyLoading}
            onChange={(event) => void selectSession(event.target.value)}
          >
            <option value="recent">Recent combined</option>
            {sessions.map((session) => (
              <option key={session.id} value={session.id}>
                {sessionLabel(session)}
              </option>
            ))}
          </select>
        </label>

        <label className="toolbar-select">
          <Filter />
          <select
            value={componentId}
            onChange={(event) => setComponentId(event.target.value)}
          >
            <option value="all">All components</option>
            {workspace.components.map((component) => (
              <option key={component.id} value={component.id}>
                {component.name}
              </option>
            ))}
          </select>
        </label>

        <label className="toolbar-search">
          <Search />
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Filter logs"
          />
        </label>

        <span className="log-result-count">
          {historyLoading ? "Loading…" : `${logs.length} lines`}
        </span>
      </div>

      {historyError ? (
        <div className="log-history-error">
          Could not load run history · {historyError}
        </div>
      ) : null}

      {logs.length > 0 ? (
        <div className="log-console" aria-label="Workspace logs">
          {logs.map((entry) => (
            <div className="log-line" key={entry.id}>
              <span
                className={
                  entry.level === "error"
                    ? "log-level log-level--error"
                    : entry.level === "warning"
                      ? "log-level log-level--warning"
                      : entry.level === "success"
                        ? "log-level log-level--success"
                        : "log-level"
                }
              >
                <Circle />
              </span>
              <span className="log-time">{entry.time}</span>
              <span className="log-component">{entry.component}</span>
              <span className="log-stream">{entry.stream}</span>
              <span className="log-message">{entry.message}</span>
            </div>
          ))}
        </div>
      ) : (
        <div className="system-empty system-empty--compact">
          <span className="system-empty__icon">
            <TerminalSquare />
          </span>
          <strong>No matching logs</strong>
          <p>
            {sourceLogs.length === 0
              ? selectedSessionId === "recent"
                ? "Run a component to see stdout, stderr, and lifecycle events here."
                : "This run session has no persisted log lines."
              : "Change the component or search filter to see more output."}
          </p>
        </div>
      )}

      <div className="log-footer">
        <span>
          Retention · {retentionDays} days · stored locally
        </span>
        <span>
          {selectedSessionId === "recent"
            ? follow
              ? "Recent stream · auto-scroll on"
              : "Recent stream · auto-scroll paused"
            : "Historical run snapshot"}
        </span>
      </div>
    </section>
  );
}
