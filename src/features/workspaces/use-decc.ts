import { useCallback, useEffect, useRef, useState } from "react";
import {
  acceptWorkspace,
  chooseEditor,
  clearProcessHistory,
  discoverEditors,
  getPreferences,
  getProcessHistory,
  inspectEnvironment,
  inspectGit,
  inspectGitHeads,
  listGitBranches,
  inspectPorts,
  inspectProcessUsage,
  inspectRuntimes,
  listOwnedProcesses,
  listPersistedWorkspaces,
  listenProcessEvents,
  openInIde,
  removePersistedWorkspace,
  runNativeWorkspace,
  savePreferences,
  setLogRetentionDays,
  setWorkspaceTrust,
  startNativeComponent,
  stopNativeComponent,
  restartNativeComponent,
  stopNativeWorkspace,
  switchGitBranch,
} from "@/services/desktop";
import { defaultPreferences } from "./defaults";
import {
  dedupeEditors,
  editorIdentityKey,
  sameEditorIdentity,
} from "./editor-utils";
import type {
  AppPreferences,
  AppSettings,
  DetectedCandidate,
  EditorSelection,
  NativeProcessEvent,
  NativeProcessHistory,
  NativeProcessStatus,
  NativeProcessUsage,
  NativeWorkspaceEnvironmentInspection,
  NativeWorkspaceGitHeadInspection,
  NativeWorkspaceGitInspection,
  NativeWorkspacePortInspection,
  NativeWorkspaceRuntimeInspection,
  PersistedWorkspace,
  Workspace,
  WorkspaceActivity,
  WorkspaceLog,
  WorkspaceSection,
} from "./model";
import { reconcileStartupPreferences } from "./startup-state";

async function loadPreferencesSafe(): Promise<{
  preferences: AppPreferences;
  error?: string;
}> {
  try {
    return { preferences: await getPreferences() };
  } catch (error) {
    return {
      preferences: defaultPreferences,
      error: error instanceof Error ? error.message : String(error),
    };
  }
}

async function loadEditorOptions(): Promise<{
  editors: EditorSelection[];
  error?: string;
}> {
  try {
    return { editors: await discoverEditors() };
  } catch (error) {
    return {
      editors: [],
      error: error instanceof Error ? error.message : String(error),
    };
  }
}

function mergeEditors(
  discovered: EditorSelection[],
  preferences: AppPreferences,
): EditorSelection[] {
  return dedupeEditors(discovered, [
    ...Object.values(preferences.workspaceEditors),
    ...(preferences.defaultEditor ? [preferences.defaultEditor] : []),
  ]);
}

function legacyEditor(
  preferences: AppPreferences,
  editors: EditorSelection[],
): EditorSelection | undefined {
  if (!preferences.defaultIde) return undefined;
  const legacyKey = editorIdentityKey({ label: preferences.defaultIde });
  return editors.find((editor) => editorIdentityKey(editor) === legacyKey);
}

function editorForWorkspace(
  preferences: AppPreferences,
  workspaceId: string,
  editors: EditorSelection[],
): EditorSelection | undefined {
  return (
    preferences.workspaceEditors[workspaceId] ??
    preferences.defaultEditor ??
    legacyEditor(preferences, editors)
  );
}

async function loadEnvironmentInspections(workspaceId?: string): Promise<{
  inspections: NativeWorkspaceEnvironmentInspection[];
  error?: string;
}> {
  try {
    return { inspections: await inspectEnvironment(workspaceId) };
  } catch (error) {
    return {
      inspections: [],
      error: error instanceof Error ? error.message : String(error),
    };
  }
}

async function loadGitInspections(workspaceId?: string): Promise<{
  inspections: NativeWorkspaceGitInspection[];
  error?: string;
}> {
  try {
    return { inspections: await inspectGit(workspaceId) };
  } catch (error) {
    return {
      inspections: [],
      error: error instanceof Error ? error.message : String(error),
    };
  }
}

async function loadGitHeadInspections(): Promise<{
  inspections: NativeWorkspaceGitHeadInspection[];
  error?: string;
}> {
  try {
    return { inspections: await inspectGitHeads() };
  } catch (error) {
    return {
      inspections: [],
      error: error instanceof Error ? error.message : String(error),
    };
  }
}

async function loadRuntimeInspections(workspaceId?: string): Promise<{
  inspections: NativeWorkspaceRuntimeInspection[];
  error?: string;
}> {
  try {
    return { inspections: await inspectRuntimes(workspaceId) };
  } catch (error) {
    return {
      inspections: [],
      error: error instanceof Error ? error.message : String(error),
    };
  }
}

function workspaceHealth(
  issues: Workspace["issues"],
  environmentVariables: Workspace["environmentVariables"],
): Workspace["health"] {
  return issues.length === 0 &&
    !environmentVariables.some((variable) => variable.status === "missing")
    ? "healthy"
    : "attention";
}

function nowTime() {
  return new Date().toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

function categoryForTechnology(technology: string) {
  if (["Next.js", "React", "Vite", "Nuxt", "Svelte", "Remix"].includes(technology)) {
    return "Web app";
  }
  if (["Go", "ASP.NET Core"].includes(technology)) {
    return "Service";
  }
  if (technology === "Docker Compose") {
    return "Infrastructure";
  }
  if (["Make", "Task", "just"].includes(technology)) {
    return "Project command";
  }
  return "Application";
}

function persistedToWorkspace(
  persisted: PersistedWorkspace,
  editor?: EditorSelection,
): Workspace {
  const executionAvailable = persisted.components.every(
    (component) => Boolean(component.program),
  );
  const issues: Workspace["issues"] = [];
  if (!persisted.trusted) {
    issues.push({
      id: "workspace-untrusted",
      title: "Workspace is not trusted",
      detail: "Review detected commands before allowing DECC to execute them.",
      tone: "warning",
    });
  }
  if (!executionAvailable) {
    issues.push({
      id: "execution-contract-missing",
      title: "Workspace needs a fresh scan",
      detail: "This saved workspace predates native process execution. Rescan and add it again before running commands.",
      tone: "warning",
    });
  }

  return {
    id: persisted.id,
    name: persisted.name,
    path: persisted.path,
    branch: "Checking Git…",
    ide: editor?.label ?? "Choose editor",
    editor,
    trusted: persisted.trusted,
    executionAvailable,
    health: issues.length === 0 ? "healthy" : "attention",
    components: persisted.components.map((component) => ({
      id: component.id,
      name: component.name,
      category: categoryForTechnology(component.technology),
      technology: component.technology,
      runtime: component.runtime,
      workingDirectory: component.relativePath,
      command: component.command,
      status: "stopped",
      expectedPorts: component.expectedPorts,
      includeInRunAll: component.includeInRunAll,
    })),
    issues,
    activity: [
      activity("system", "Workspace loaded from local storage", "info"),
    ],
    logs: [],
    environmentFiles: [],
    environmentVariables: [],
    git: {
      state: "notInspected",
      branch: "Checking Git…",
      ahead: 0,
      behind: 0,
      changes: [],
      commits: [],
    },
  };
}

function activity(source: string, message: string, level: WorkspaceActivity["level"]): WorkspaceActivity {
  return {
    id: crypto.randomUUID(),
    time: nowTime(),
    source,
    message,
    level,
  };
}

const MAX_VISIBLE_NATIVE_LOGS = 2000;

function processClock(timestampMs: number, withMillis = false) {
  const date = new Date(timestampMs);
  const base = date.toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  });
  return withMillis
    ? `${base}.${String(date.getMilliseconds()).padStart(3, "0")}`
    : base.slice(0, 5);
}

function nativeEventId(event: NativeProcessEvent) {
  const payload = [
    event.workspaceId,
    event.componentId,
    event.kind,
    event.stream ?? "system",
    event.timestampMs,
    event.pid ?? "",
    event.exitCode ?? "",
    event.message,
  ].join("\u001f");

  let hash = 2166136261;
  for (let index = 0; index < payload.length; index += 1) {
    hash ^= payload.charCodeAt(index);
    hash = Math.imul(hash, 16777619);
  }
  return `native-log-${event.timestampMs}-${(hash >>> 0).toString(16)}`;
}

function nativeEventLog(event: NativeProcessEvent): WorkspaceLog {
  return {
    id: nativeEventId(event),
    time: processClock(event.timestampMs, true),
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

function applyNativeProcessHistory(
  workspace: Workspace,
  history: NativeProcessHistory,
): Workspace {
  const componentIds = new Set(workspace.components.map((component) => component.id));
  const logs = history.events
    .filter(
      (event) =>
        event.workspaceId === workspace.id && componentIds.has(event.componentId),
    )
    .map(nativeEventLog)
    .slice(0, MAX_VISIBLE_NATIVE_LOGS);
  return { ...workspace, logs };
}

function applyNativeProcessStatus(
  workspace: Workspace,
  status: NativeProcessStatus,
): Workspace {
  if (workspace.id !== status.workspaceId) return workspace;
  return {
    ...workspace,
    components: workspace.components.map((component) =>
      component.id === status.componentId
        ? {
            ...component,
            status: "running",
            pid: status.pid,
            startedAt: processClock(status.startedAtMs),
            lastExitCode: undefined,
          }
        : component,
    ),
  };
}

function applyNativeProcessUsage(
  workspace: Workspace,
  usage: NativeProcessUsage,
): Workspace {
  if (workspace.id !== usage.workspaceId) return workspace;
  return {
    ...workspace,
    components: workspace.components.map((component) =>
      component.id === usage.componentId && component.pid === usage.pid
        ? {
            ...component,
            cpuPercent: usage.cpuPercent,
            memoryBytes: usage.memoryBytes,
          }
        : component,
    ),
  };
}

function applyNativeProcessEvent(
  workspace: Workspace,
  event: NativeProcessEvent,
): Workspace {
  if (workspace.id !== event.workspaceId) return workspace;
  const target = workspace.components.find(
    (component) => component.id === event.componentId,
  );
  if (!target) return workspace;

  const eventLog = nativeEventLog(event);
  const logs = [
    eventLog,
    ...workspace.logs.filter((entry) => entry.id !== eventLog.id),
  ].slice(0, MAX_VISIBLE_NATIVE_LOGS);

  if (event.kind === "log") {
    return { ...workspace, logs };
  }

  if (event.kind === "starting") {
    return {
      ...workspace,
      logs,
      activity: [
        activity(event.componentName, "Starting native process…", "info"),
        ...workspace.activity,
      ],
      components: workspace.components.map((component) =>
        component.id === event.componentId
          ? { ...component, status: "starting" }
          : component,
      ),
    };
  }

  if (event.kind === "started") {
    const issues = workspace.issues.filter(
      (issue) => issue.id !== `${event.componentId}-failed`,
    );
    return {
      ...workspace,
      logs,
      issues,
      health: workspaceHealth(issues, workspace.environmentVariables),
      activity: [
        activity(event.componentName, `Running as PID ${event.pid ?? "?"}`, "success"),
        ...workspace.activity,
      ],
      components: workspace.components.map((component) =>
        component.id === event.componentId
          ? {
              ...component,
              status: "running",
              pid: event.pid,
              startedAt: processClock(event.timestampMs),
              lastExitCode: undefined,
            }
          : component,
      ),
    };
  }

  if (event.kind === "stopping") {
    return {
      ...workspace,
      logs,
      activity: [
        activity(event.componentName, "Stopping DECC-owned process…", "info"),
        ...workspace.activity,
      ],
      components: workspace.components.map((component) =>
        component.id === event.componentId
          ? { ...component, status: "stopping" }
          : component,
      ),
    };
  }

  if (event.kind === "exited") {
    const expectedStop = target.status === "stopping";
    const failed =
      !expectedStop && event.exitCode !== undefined && event.exitCode !== 0;
    const issues = workspace.issues.filter(
      (issue) => issue.id !== `${event.componentId}-failed`,
    );
    if (failed) {
      issues.unshift({
        id: `${event.componentId}-failed`,
        title: `${event.componentName} exited unexpectedly`,
        detail: event.message,
        tone: "danger",
      });
    }
    return {
      ...workspace,
      logs,
      issues,
      health: workspaceHealth(issues, workspace.environmentVariables),
      activity: [
        activity(
          event.componentName,
          expectedStop ? "Stopped" : event.message,
          failed ? "error" : "info",
        ),
        ...workspace.activity,
      ],
      components: workspace.components.map((component) =>
        component.id === event.componentId
          ? {
              ...component,
              status: failed ? "error" : "stopped",
              pid: undefined,
              startedAt: undefined,
              cpuPercent: undefined,
              memoryBytes: undefined,
              lastExitCode: expectedStop ? undefined : event.exitCode,
            }
          : component,
      ),
    };
  }

  const issues = [
    {
      id: `${event.componentId}-failed`,
      title: `${event.componentName} process error`,
      detail: event.message,
      tone: "danger" as const,
    },
    ...workspace.issues.filter(
      (issue) => issue.id !== `${event.componentId}-failed`,
    ),
  ];
  return {
    ...workspace,
    logs,
    issues,
    health: "attention",
    activity: [
      activity(event.componentName, event.message, "error"),
      ...workspace.activity,
    ],
    components: workspace.components.map((component) =>
      component.id === event.componentId
        ? {
            ...component,
            status: "error",
            pid: undefined,
            startedAt: undefined,
            cpuPercent: undefined,
            memoryBytes: undefined,
          }
        : component,
    ),
  };
}

function applyNativePortInspection(
  workspace: Workspace,
  inspection: NativeWorkspacePortInspection,
): Workspace {
  if (workspace.id !== inspection.workspaceId) return workspace;

  const portIssues: Workspace["issues"] = [];
  const components = workspace.components.map((component) => {
    const componentInspection = inspection.components.find(
      (item) => item.componentId === component.id,
    );
    if (!componentInspection) return component;

    const ownedListener = componentInspection.listeners.find(
      (listener) => listener.origin === "owned",
    );
    const externalListener = componentInspection.listeners.find(
      (listener) => listener.origin === "external",
    );
    const conflictListener = componentInspection.listeners.find(
      (listener) =>
        listener.origin === "external" && listener.matchesExpectedPort,
    );
    const primaryListener = ownedListener ?? externalListener;

    if (componentInspection.hasConflict && conflictListener) {
      const process = conflictListener.processName ?? "External process";
      const pid = conflictListener.pid ? ` · PID ${conflictListener.pid}` : "";
      const cwd = conflictListener.cwd ? ` · ${conflictListener.cwd}` : "";
      portIssues.push({
        id: `port-conflict:${component.id}`,
        title: `Port ${conflictListener.port} is already in use`,
        detail: `${process}${pid}${cwd}`,
        tone: "danger",
      });
    }

    let status = component.status;
    let pid = component.pid;
    if (ownedListener) {
      if (!["starting", "stopping"].includes(status)) status = "running";
    } else if (componentInspection.externalRunning) {
      if (!["running", "starting", "stopping"].includes(status)) {
        status = "external";
        pid = externalListener?.pid;
      }
    } else if (status === "external") {
      status = "stopped";
      pid = undefined;
    }

    return {
      ...component,
      status,
      pid,
      port: primaryListener?.port,
      expectedPorts: componentInspection.expectedPorts,
      portOrigin: primaryListener?.origin,
      portConflict: componentInspection.hasConflict,
      portProcessName: primaryListener?.processName,
      portProcessCwd: primaryListener?.cwd,
    };
  });

  const issues = [
    ...portIssues,
    ...workspace.issues.filter((issue) => !issue.id.startsWith("port-conflict:")),
  ];

  return {
    ...workspace,
    components,
    issues,
    health: workspaceHealth(issues, workspace.environmentVariables),
  };
}

function applyNativeEnvironmentInspection(
  workspace: Workspace,
  inspection: NativeWorkspaceEnvironmentInspection,
): Workspace {
  if (workspace.id !== inspection.workspaceId) return workspace;

  const environmentFiles = inspection.files.map((file) => ({
    name: file.name,
    path: file.relativePath,
    kind: file.kind,
    componentId: file.componentId,
    scope: file.componentName ?? "Workspace",
  }));
  const environmentVariables = inspection.variables.map((variable) => {
    const sourcePaths =
      variable.status === "present"
        ? variable.sourcePaths
        : variable.requirementPaths;
    return {
      name: variable.name,
      status: variable.status,
      source: sourcePaths.length > 0 ? sourcePaths.join(", ") : undefined,
      requiredBy: variable.componentName ?? "Workspace",
      componentId: variable.componentId,
    };
  });

  const issues = [
    ...(inspection.error
      ? [
          {
            id: "environment:inspection-error",
            title: "Environment inspection failed",
            detail: inspection.error,
            tone: "warning" as const,
          },
        ]
      : []),
    ...workspace.issues.filter(
      (issue) => issue.id !== "environment:inspection-error",
    ),
  ];

  return {
    ...workspace,
    issues,
    environmentFiles,
    environmentVariables,
    health: workspaceHealth(issues, environmentVariables),
  };
}

function applyNativeGitHeadInspection(
  workspace: Workspace,
  inspection: NativeWorkspaceGitHeadInspection,
): Workspace {
  if (workspace.id !== inspection.workspaceId) return workspace;
  if (workspace.git.state !== "notInspected") return workspace;

  if (inspection.state === "ready" && inspection.branch) {
    return {
      ...workspace,
      branch: inspection.branch,
      git: {
        ...workspace.git,
        state: "notInspected",
        branch: inspection.branch,
        detached: inspection.detached,
        error: undefined,
      },
    };
  }

  const branch =
    inspection.state === "notRepository"
      ? "No Git repository"
      : inspection.state === "unavailable"
        ? "Git unavailable"
        : "Git error";

  return {
    ...workspace,
    branch,
    git: {
      ...workspace.git,
      state: "notInspected",
      branch,
      detached: false,
      error: inspection.error,
    },
  };
}

function applyNativeGitInspection(
  workspace: Workspace,
  inspection: NativeWorkspaceGitInspection,
): Workspace {
  if (workspace.id !== inspection.workspaceId) return workspace;

  if (inspection.state === "ready") {
    const branch = inspection.branch ?? "Detached";
    return {
      ...workspace,
      branch,
      git: {
        state: "ready",
        repositoryRoot: inspection.repositoryRoot,
        branch,
        detached: inspection.detached,
        upstream: inspection.upstream,
        ahead: inspection.ahead,
        behind: inspection.behind,
        changes: inspection.changes,
        changesTruncated: inspection.changesTruncated,
        commits: inspection.commits,
      },
    };
  }

  const branch =
    inspection.state === "notRepository"
      ? "No Git repository"
      : inspection.state === "unavailable"
        ? "Git unavailable"
        : "Git error";

  return {
    ...workspace,
    branch,
    git: {
      state: inspection.state,
      repositoryRoot: inspection.repositoryRoot,
      branch,
      detached: false,
      ahead: 0,
      behind: 0,
      changes: [],
      commits: [],
      error: inspection.error,
    },
  };
}

function applyGitInspectionError(workspace: Workspace, error: string): Workspace {
  return {
    ...workspace,
    branch: "Git unavailable",
    git: {
      state: "error",
      branch: "Git unavailable",
      detached: false,
      ahead: 0,
      behind: 0,
      changes: [],
      commits: [],
      error,
    },
  };
}

function applyNativeRuntimeInspection(
  workspace: Workspace,
  inspection: NativeWorkspaceRuntimeInspection,
): Workspace {
  if (workspace.id !== inspection.workspaceId) return workspace;

  const runtimeIssues: Workspace["issues"] = [];
  const components = workspace.components.map((component) => {
    const componentInspection = inspection.components.find(
      (item) => item.componentId === component.id,
    );
    if (!componentInspection) return component;

    for (const diagnostic of componentInspection.diagnostics) {
      if (!diagnostic.blocking) continue;
      runtimeIssues.push({
        id: `runtime:${component.id}:${diagnostic.key}`,
        title:
          diagnostic.key === "workspace-path"
            ? "Workspace folder is unavailable"
            : diagnostic.key === "make-command-shape"
              ? "Make run target needs an app selection"
              : diagnostic.status === "missing"
                ? `${diagnostic.label} isn't available to DECC`
                : diagnostic.status === "incompatible"
                  ? `${diagnostic.label} version is incompatible`
                  : `${diagnostic.label} needs a runtime selection`,
        detail: diagnostic.detail,
        tone: "danger",
        componentId: component.id,
        action:
          diagnostic.key === "make-command-shape" ||
          diagnostic.key === "workspace-path"
            ? "rescan"
            : "inspect",
      });
    }

    return {
      ...component,
      runtimeReady: componentInspection.ready,
      runtimeBlockingReason: componentInspection.blockingMessage,
      runtimeDiagnostics: componentInspection.diagnostics,
    };
  });

  const issues = [
    ...runtimeIssues,
    ...workspace.issues.filter((issue) => !issue.id.startsWith("runtime:")),
  ];

  return {
    ...workspace,
    components,
    issues,
    health: workspaceHealth(issues, workspace.environmentVariables),
  };
}

export function useDecc() {
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [editors, setEditors] = useState<EditorSelection[]>([]);
  const [preferences, setPreferences] =
    useState<AppPreferences>(defaultPreferences);
  const preferencesRef = useRef<AppPreferences>(defaultPreferences);
  const preferencesSaveChainRef = useRef<Promise<void>>(Promise.resolve());
  const [preferencesLoaded, setPreferencesLoaded] = useState(false);
  const settings: AppSettings = preferences;
  const [workspaceLoadError, setWorkspaceLoadError] = useState<string>();
  const [processActionError, setProcessActionError] = useState<string>();

  const commitPreferences = useCallback(
    (update: (current: AppPreferences) => AppPreferences) => {
      const next = update(preferencesRef.current);
      preferencesRef.current = next;
      setPreferences(next);

      preferencesSaveChainRef.current = preferencesSaveChainRef.current
        .catch(() => undefined)
        .then(async () => {
          try {
            await savePreferences(next);
            setProcessActionError((current) =>
              current?.startsWith("Could not save preferences") ? undefined : current,
            );
          } catch (error) {
            setProcessActionError(
              `Could not save preferences · ${error instanceof Error ? error.message : String(error)}`,
            );
          }
        });
    },
    [],
  );

  useEffect(() => {
    let cancelled = false;

    void Promise.all([
      loadPreferencesSafe(),
      loadEditorOptions(),
      listPersistedWorkspaces(),
      listOwnedProcesses(),
    ])
      .then(([preferencesResult, editorResult, persisted, ownedProcesses]) => {
        if (cancelled) return;

        let loadedPreferences = preferencesResult.preferences;
        const discoveredEditors = editorResult.editors;
        const migratedDefault =
          loadedPreferences.defaultEditor ??
          legacyEditor(loadedPreferences, discoveredEditors);
        let preferencesChanged = false;
        if (!loadedPreferences.defaultEditor && migratedDefault) {
          loadedPreferences = {
            ...loadedPreferences,
            defaultEditor: migratedDefault,
            defaultIde: undefined,
          };
          preferencesChanged = true;
        }

        if (loadedPreferences.defaultEditor) {
          const workspaceEditors = Object.fromEntries(
            Object.entries(loadedPreferences.workspaceEditors).filter(
              ([, editor]) =>
                !sameEditorIdentity(editor, loadedPreferences.defaultEditor),
            ),
          );
          if (
            Object.keys(workspaceEditors).length !==
            Object.keys(loadedPreferences.workspaceEditors).length
          ) {
            loadedPreferences = { ...loadedPreferences, workspaceEditors };
            preferencesChanged = true;
          }
        }

        const reconciled = reconcileStartupPreferences(
          loadedPreferences,
          persisted.map((workspace) => workspace.id),
        );
        loadedPreferences = reconciled.preferences;
        preferencesChanged ||= reconciled.changed;

        if (preferencesChanged) {
          void savePreferences(loadedPreferences).catch(() => undefined);
        }

        const editorOptions = mergeEditors(discoveredEditors, loadedPreferences);
        preferencesRef.current = loadedPreferences;
        setPreferences(loadedPreferences);
        setEditors(editorOptions);
        setPreferencesLoaded(true);
        void setLogRetentionDays(loadedPreferences.logRetentionDays).catch((error) => {
          if (cancelled) return;
          setProcessActionError(
            `Could not apply log retention · ${error instanceof Error ? error.message : String(error)}`,
          );
        });

        const diagnosticErrors = [
          preferencesResult.error
            ? `Preferences unavailable · ${preferencesResult.error}`
            : undefined,
          editorResult.error
            ? `Editor discovery unavailable · ${editorResult.error}`
            : undefined,
        ].filter((message): message is string => Boolean(message));
        if (diagnosticErrors.length > 0) {
          setProcessActionError(diagnosticErrors.join(" · "));
        }

        const restored = persisted.map((workspace) => {
          let view = persistedToWorkspace(
            workspace,
            editorForWorkspace(loadedPreferences, workspace.id, editorOptions),
          );
          for (const process of ownedProcesses) {
            view = applyNativeProcessStatus(view, process);
          }
          return view;
        });

        setWorkspaces(restored);

        // Populate lightweight branch/repository labels once after the initial
        // workspace list is painted. This does not load status, history, or
        // changes, and applyNativeGitHeadInspection never overwrites a
        // workspace that has already received a full Git inspection.
        window.setTimeout(() => {
          if (cancelled) return;
          void loadGitHeadInspections().then((result) => {
            if (cancelled) return;
            if (result.inspections.length === 0) {
              if (!result.error) return;
              setWorkspaces((current) =>
                current.map((workspace) =>
                  workspace.git.state === "notInspected"
                    ? {
                        ...workspace,
                        branch: "Git check unavailable",
                        git: {
                          ...workspace.git,
                          branch: "Git check unavailable",
                          error: result.error,
                        },
                      }
                    : workspace,
                ),
              );
              return;
            }

            const byWorkspace = new Map(
              result.inspections.map((inspection) => [
                inspection.workspaceId,
                inspection,
              ]),
            );
            setWorkspaces((current) =>
              current.map((workspace) => {
                const inspection = byWorkspace.get(workspace.id);
                return inspection
                  ? applyNativeGitHeadInspection(workspace, inspection)
                  : workspace;
              }),
            );
          });
        }, 120);

      })
      .catch((error) => {
        if (cancelled) return;
        setPreferencesLoaded(true);
        setWorkspaceLoadError(error instanceof Error ? error.message : String(error));
      });

    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;

    void listenProcessEvents((event) => {
      if (cancelled) return;
      setWorkspaces((current) =>
        current.map((workspace) => applyNativeProcessEvent(workspace, event)),
      );
    })
      .then((stopListening) => {
        if (cancelled) {
          stopListening();
          return;
        }
        unlisten = stopListening;
      })
      .catch((error) => {
        if (!cancelled) {
          setProcessActionError(
            `Could not listen for native process events · ${error instanceof Error ? error.message : String(error)}`,
          );
        }
      });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  const updateWorkspace = useCallback(
    (workspaceId: string, transform: (workspace: Workspace) => Workspace) => {
      setWorkspaces((current) =>
        current.map((workspace) =>
          workspace.id === workspaceId ? transform(workspace) : workspace,
        ),
      );
    },
    [],
  );

  const refreshWorkspacePorts = useCallback(
    async (workspaceId: string) => {
      try {
        const inspections = await inspectPorts(workspaceId);
        const inspection = inspections.find(
          (item) => item.workspaceId === workspaceId,
        );
        if (inspection) {
          updateWorkspace(workspaceId, (workspace) =>
            applyNativePortInspection(workspace, inspection),
          );
        }
        setProcessActionError((current) =>
          current?.startsWith("Port inspection unavailable") ? undefined : current,
        );
      } catch (error) {
        setProcessActionError(
          `Port inspection unavailable · ${error instanceof Error ? error.message : String(error)}`,
        );
      }
    },
    [updateWorkspace],
  );

  const refreshWorkspaceUsage = useCallback(
    async (workspaceId: string) => {
      try {
        const usages = await inspectProcessUsage(workspaceId);
        if (usages.length === 0) return;
        updateWorkspace(workspaceId, (workspace) =>
          usages.reduce(applyNativeProcessUsage, workspace),
        );
      } catch (error) {
        setProcessActionError(
          `Process metrics unavailable · ${error instanceof Error ? error.message : String(error)}`,
        );
      }
    },
    [updateWorkspace],
  );

  const hydrateWorkspace = useCallback(
    async (workspaceId: string) => {
      const [historyResult, runtimeResult, environmentResult, portResult] =
        await Promise.all([
          getProcessHistory(workspaceId, MAX_VISIBLE_NATIVE_LOGS)
            .then((history) => ({ history }))
            .catch((error) => ({
              error: error instanceof Error ? error.message : String(error),
            })),
          loadRuntimeInspections(workspaceId),
          loadEnvironmentInspections(workspaceId),
          inspectPorts(workspaceId)
            .then((inspections) => ({ inspections }))
            .catch((error) => ({
              inspections: [] as NativeWorkspacePortInspection[],
              error: error instanceof Error ? error.message : String(error),
            })),
        ]);

      const runtimeInspection = runtimeResult.inspections.find(
        (inspection) => inspection.workspaceId === workspaceId,
      );
      const environmentInspection = environmentResult.inspections.find(
        (inspection) => inspection.workspaceId === workspaceId,
      );
      const portInspection = portResult.inspections.find(
        (inspection) => inspection.workspaceId === workspaceId,
      );

      updateWorkspace(workspaceId, (workspace) => {
        let view = workspace;
        if ("history" in historyResult && historyResult.history) {
          view = applyNativeProcessHistory(view, historyResult.history);
        }
        if (runtimeInspection) {
          view = applyNativeRuntimeInspection(view, runtimeInspection);
        }
        if (environmentInspection) {
          view = applyNativeEnvironmentInspection(view, environmentInspection);
        }
        if (portInspection) {
          view = applyNativePortInspection(view, portInspection);
        }
        return view;
      });

      const errors = [
        "error" in historyResult && historyResult.error
          ? `Local log history unavailable · ${historyResult.error}`
          : undefined,
        runtimeResult.error
          ? `Runtime inspection unavailable · ${runtimeResult.error}`
          : undefined,
        environmentResult.error
          ? `Environment inspection unavailable · ${environmentResult.error}`
          : undefined,
        environmentInspection?.error
          ? `Environment inspection unavailable · ${environmentInspection.error}`
          : undefined,
        "error" in portResult && portResult.error
          ? `Port inspection unavailable · ${portResult.error}`
          : undefined,
      ].filter((message): message is string => Boolean(message));

      if (errors.length > 0) {
        setProcessActionError(errors.join(" · "));
      }
    },
    [updateWorkspace],
  );

  const startComponent = useCallback(
    async (workspaceId: string, componentId: string) => {
      setProcessActionError(undefined);
      try {
        await startNativeComponent(workspaceId, componentId);
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        setProcessActionError(`Could not start component · ${message}`);
        updateWorkspace(workspaceId, (workspace) => ({
          ...workspace,
          health: "attention",
          issues: [
            {
              id: `${componentId}-failed`,
              title: "Component could not start",
              detail: message,
              tone: "danger",
            },
            ...workspace.issues.filter(
              (issue) => issue.id !== `${componentId}-failed`,
            ),
          ],
          activity: [
            activity("system", message, "error"),
            ...workspace.activity,
          ],
          components: workspace.components.map((component) =>
            component.id === componentId
              ? { ...component, status: "error" }
              : component,
          ),
        }));
      }
    },
    [updateWorkspace],
  );

  const stopComponent = useCallback(
    async (workspaceId: string, componentId: string) => {
      setProcessActionError(undefined);
      try {
        await stopNativeComponent(workspaceId, componentId);
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        setProcessActionError(`Could not stop component · ${message}`);
      }
    },
    [],
  );

  const restartComponent = useCallback(
    async (workspaceId: string, componentId: string) => {
      setProcessActionError(undefined);
      try {
        await restartNativeComponent(workspaceId, componentId);
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        setProcessActionError(`Could not restart component · ${message}`);
      }
    },
    [],
  );

  const runAll = useCallback(
    async (workspaceId: string) => {
      setProcessActionError(undefined);
      try {
        const result = await runNativeWorkspace(workspaceId);
        if (result.errors.length > 0) {
          const message = result.errors.map((error) => error.message).join(" · ");
          setProcessActionError(`Run all completed with errors · ${message}`);
          updateWorkspace(workspaceId, (workspace) => ({
            ...workspace,
            health: "attention",
            issues: [
              ...result.errors.map((error) => ({
                id: `${error.componentId}-failed`,
                title: "Component could not start",
                detail: error.message,
                tone: "danger" as const,
              })),
              ...workspace.issues.filter(
                (issue) =>
                  !result.errors.some(
                    (error) => issue.id === `${error.componentId}-failed`,
                  ),
              ),
            ],
          }));
        }
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        setProcessActionError(`Could not run workspace · ${message}`);
      }
    },
    [updateWorkspace],
  );

  const stopAll = useCallback(async (workspaceId: string) => {
    setProcessActionError(undefined);
    try {
      const result = await stopNativeWorkspace(workspaceId);
      if (result.errors.length > 0) {
        setProcessActionError(
          `Stop all completed with errors · ${result.errors
            .map((error) => error.message)
            .join(" · ")}`,
        );
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setProcessActionError(`Could not stop workspace · ${message}`);
    }
  }, []);

  const loadLogHistory = useCallback(
    (workspaceId: string, sessionId?: string) =>
      getProcessHistory(workspaceId, MAX_VISIBLE_NATIVE_LOGS, sessionId),
    [],
  );

  const clearLogs = useCallback(
    (workspaceId: string) => {
      updateWorkspace(workspaceId, (workspace) => ({
        ...workspace,
        logs: [],
        activity: [
          activity("system", "Log history cleared", "info"),
          ...workspace.activity,
        ],
      }));

      void clearProcessHistory(workspaceId).catch((error) => {
        setProcessActionError(
          `Could not clear local log history · ${error instanceof Error ? error.message : String(error)}`,
        );
      });
    },
    [updateWorkspace],
  );

  const trustWorkspace = useCallback(
    async (workspaceId: string) => {
      const persisted = await setWorkspaceTrust(workspaceId, true);
      updateWorkspace(workspaceId, (workspace) => {
        const issues = workspace.issues.filter(
          (issue) => issue.id !== "workspace-untrusted",
        );
        return {
          ...workspace,
          trusted: persisted.trusted,
          health:
            issues.length === 0 &&
            !workspace.environmentVariables.some(
              (variable) => variable.status === "missing",
            )
              ? "healthy"
              : "attention",
          issues,
          activity: [
            activity("system", "Workspace trusted for command execution", "success"),
            ...workspace.activity,
          ],
        };
      });
    },
    [updateWorkspace],
  );

  const acceptDetectedWorkspace = useCallback(
    async (path: string, candidates: DetectedCandidate[]) => {
      const candidateIds = candidates
        .filter((candidate) => candidate.included)
        .map((candidate) => candidate.id);
      const persisted = await acceptWorkspace(path, candidateIds);
      const ignoredCandidateIds = candidates
        .filter((candidate) => !candidate.included)
        .map((candidate) => candidate.id);
      commitPreferences((current) => {
        const ignoredWorkspaceCandidates = {
          ...current.ignoredWorkspaceCandidates,
        };
        if (ignoredCandidateIds.length > 0) {
          ignoredWorkspaceCandidates[persisted.id] = ignoredCandidateIds;
        } else {
          delete ignoredWorkspaceCandidates[persisted.id];
        }
        return { ...current, ignoredWorkspaceCandidates };
      });

      let workspace = persistedToWorkspace(
        persisted,
        editorForWorkspace(preferencesRef.current, persisted.id, editors),
      );
      const [runtimeResult, environmentResult] = await Promise.all([
        loadRuntimeInspections(persisted.id),
        loadEnvironmentInspections(persisted.id),
      ]);
      const runtimeInspection = runtimeResult.inspections.find(
        (inspection) => inspection.workspaceId === persisted.id,
      );
      if (runtimeInspection) {
        workspace = applyNativeRuntimeInspection(workspace, runtimeInspection);
      }
      const environmentInspection = environmentResult.inspections.find(
        (inspection) => inspection.workspaceId === persisted.id,
      );
      if (environmentInspection) {
        workspace = applyNativeEnvironmentInspection(
          workspace,
          environmentInspection,
        );
      }
      const diagnosticErrors = [
        runtimeResult.error
          ? `Runtime inspection unavailable · ${runtimeResult.error}`
          : undefined,
        environmentResult.error
          ? `Environment inspection unavailable · ${environmentResult.error}`
          : undefined,
        environmentInspection?.error
          ? `Environment inspection unavailable · ${environmentInspection.error}`
          : undefined,
      ].filter((message): message is string => Boolean(message));
      if (diagnosticErrors.length > 0) {
        setProcessActionError(diagnosticErrors.join(" · "));
      }

      setWorkspaces((current) => {
        const index = current.findIndex((item) => item.id === persisted.id);
        if (index === -1) return [...current, workspace];
        return current.map((item) => (item.id === persisted.id ? workspace : item));
      });

      void loadGitInspections(persisted.id).then((result) => {
        const inspection = result.inspections.find(
          (item) => item.workspaceId === persisted.id,
        );
        if (!inspection) return;
        setWorkspaces((current) =>
          current.map((item) =>
            item.id === persisted.id
              ? applyNativeGitInspection(item, inspection)
              : item,
          ),
        );
      });

      return persisted.id;
    },
    [commitPreferences, editors],
  );

  const refreshGit = useCallback(
    async (workspaceId: string) => {
      const result = await loadGitInspections(workspaceId);
      if (result.error) {
        setProcessActionError(`Git inspection unavailable · ${result.error}`);
        updateWorkspace(workspaceId, (workspace) =>
          applyGitInspectionError(workspace, result.error!),
        );
        return;
      }

      const inspection = result.inspections.find(
        (item) => item.workspaceId === workspaceId,
      );
      if (!inspection) return;
      setProcessActionError((current) =>
        current?.startsWith("Git inspection unavailable") ? undefined : current,
      );
      updateWorkspace(workspaceId, (workspace) =>
        applyNativeGitInspection(workspace, inspection),
      );
    },
    [updateWorkspace],
  );

  const loadWorkspaceBranches = useCallback(async (workspaceId: string) => {
    try {
      return await listGitBranches(workspaceId);
    } catch (error) {
      setProcessActionError(
        `Git branches unavailable · ${error instanceof Error ? error.message : String(error)}`,
      );
      return [];
    }
  }, []);

  const changeGitBranch = useCallback(
    async (workspaceId: string, branch: string) => {
      try {
        const inspection = await switchGitBranch(workspaceId, branch);
        updateWorkspace(workspaceId, (workspace) =>
          applyNativeGitInspection(workspace, inspection),
        );
        setProcessActionError((current) =>
          current?.startsWith("Git branch") ? undefined : current,
        );
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        setProcessActionError(`Git branch switch failed · ${message}`);
        throw error;
      }
    },
    [updateWorkspace],
  );

  const updateSettings = useCallback(
    (patch: Partial<AppSettings>) => {
      commitPreferences((current) => ({
        ...current,
        ...patch,
        defaultIde: patch.defaultEditor ? undefined : current.defaultIde,
      }));
      if (patch.logRetentionDays !== undefined) {
        void setLogRetentionDays(patch.logRetentionDays).catch((error) => {
          setProcessActionError(
            `Could not apply log retention · ${error instanceof Error ? error.message : String(error)}`,
          );
        });
      }
      if (patch.defaultEditor) {
        setEditors((current) =>
          mergeEditors(current, {
            ...preferencesRef.current,
            defaultEditor: patch.defaultEditor,
          }),
        );
        setWorkspaces((current) =>
          current.map((workspace) => {
            if (preferencesRef.current.workspaceEditors[workspace.id]) {
              return workspace;
            }
            return {
              ...workspace,
              ide: patch.defaultEditor?.label ?? "Choose editor",
              editor: patch.defaultEditor,
            };
          }),
        );
      }
    },
    [commitPreferences],
  );

  const refreshEditors = useCallback(async () => {
    const discovered = await discoverEditors();
    setEditors(mergeEditors(discovered, preferencesRef.current));
    return discovered;
  }, []);

  const chooseCustomEditor = useCallback(async () => {
    const editor = await chooseEditor();
    if (!editor) return null;
    setEditors((current) =>
      mergeEditors([...current, editor], preferencesRef.current),
    );
    return editor;
  }, []);

  const setWorkspaceEditor = useCallback(
    (workspaceId: string, editor: EditorSelection | undefined) => {
      const normalizedEditor =
        editor &&
        sameEditorIdentity(editor, preferencesRef.current.defaultEditor)
          ? undefined
          : editor;

      commitPreferences((current) => {
        const workspaceEditors = { ...current.workspaceEditors };
        if (normalizedEditor) {
          workspaceEditors[workspaceId] = normalizedEditor;
        } else {
          delete workspaceEditors[workspaceId];
        }
        return { ...current, workspaceEditors };
      });

      const effective =
        normalizedEditor ??
        preferencesRef.current.defaultEditor;
      setWorkspaces((current) =>
        current.map((workspace) =>
          workspace.id === workspaceId
            ? {
                ...workspace,
                ide: effective?.label ?? "Choose editor",
                editor: effective,
              }
            : workspace,
        ),
      );
      if (normalizedEditor) {
        setEditors((current) =>
          mergeEditors([...current, normalizedEditor], preferencesRef.current),
        );
      }
    },
    [commitPreferences],
  );

  const recordWorkspaceSelection = useCallback(
    (workspaceId: string) => {
      commitPreferences((current) => ({
        ...current,
        lastWorkspaceId: workspaceId,
        recentWorkspaceIds: [
          workspaceId,
          ...current.recentWorkspaceIds.filter((id) => id !== workspaceId),
        ].slice(0, 100),
      }));
    },
    [commitPreferences],
  );

  const togglePinnedWorkspace = useCallback(
    (workspaceId: string) => {
      commitPreferences((current) => ({
        ...current,
        pinnedWorkspaceIds: current.pinnedWorkspaceIds.includes(workspaceId)
          ? current.pinnedWorkspaceIds.filter((id) => id !== workspaceId)
          : [...current.pinnedWorkspaceIds, workspaceId].slice(0, 100),
      }));
    },
    [commitPreferences],
  );

  const toggleWorkspaceSectionMuted = useCallback(
    (workspaceId: string, section: WorkspaceSection) => {
      commitPreferences((current) => {
        const mutedWorkspaceSections = { ...current.mutedWorkspaceSections };
        const currentSections = mutedWorkspaceSections[workspaceId] ?? [];
        const nextSections = currentSections.includes(section)
          ? currentSections.filter((item) => item !== section)
          : [...currentSections, section];

        if (nextSections.length > 0) {
          mutedWorkspaceSections[workspaceId] = nextSections;
        } else {
          delete mutedWorkspaceSections[workspaceId];
        }
        return { ...current, mutedWorkspaceSections };
      });
    },
    [commitPreferences],
  );

  const ignoreWorkspaceIssue = useCallback(
    (workspaceId: string, issueId: string) => {
      commitPreferences((current) => {
        const ignoredWorkspaceIssues = { ...current.ignoredWorkspaceIssues };
        const currentIssues = ignoredWorkspaceIssues[workspaceId] ?? [];
        if (!currentIssues.includes(issueId)) {
          ignoredWorkspaceIssues[workspaceId] = [...currentIssues, issueId].slice(0, 200);
        }
        return { ...current, ignoredWorkspaceIssues };
      });
    },
    [commitPreferences],
  );

  const restoreWorkspaceIssues = useCallback(
    (workspaceId: string) => {
      commitPreferences((current) => {
        const ignoredWorkspaceIssues = { ...current.ignoredWorkspaceIssues };
        delete ignoredWorkspaceIssues[workspaceId];
        return { ...current, ignoredWorkspaceIssues };
      });
    },
    [commitPreferences],
  );

  const openWorkspaceInIde = useCallback(
    async (
      workspaceId: string,
      options: { componentId?: string; relativePath?: string } = {},
    ) => {
      const workspace = workspaces.find((item) => item.id === workspaceId);
      if (!workspace) {
        throw new Error("Workspace was not found.");
      }
      if (!workspace.editor) {
        throw new Error("Choose an editor for this workspace first.");
      }
      await openInIde(workspaceId, workspace.editor, options);
    },
    [workspaces],
  );

  const removeWorkspace = useCallback(
    async (workspaceId: string) => {
      const removed = await removePersistedWorkspace(workspaceId);
      if (!removed) {
        throw new Error("Workspace was not found in local storage.");
      }

      setWorkspaces((current) =>
        current.filter((workspace) => workspace.id !== workspaceId),
      );
      commitPreferences((current) => {
        const recentWorkspaceIds = current.recentWorkspaceIds.filter(
          (id) => id !== workspaceId,
        );
        const workspaceEditors = { ...current.workspaceEditors };
        const mutedWorkspaceSections = { ...current.mutedWorkspaceSections };
        const ignoredWorkspaceIssues = { ...current.ignoredWorkspaceIssues };
        const ignoredWorkspaceCandidates = {
          ...current.ignoredWorkspaceCandidates,
        };
        delete workspaceEditors[workspaceId];
        delete mutedWorkspaceSections[workspaceId];
        delete ignoredWorkspaceIssues[workspaceId];
        delete ignoredWorkspaceCandidates[workspaceId];
        return {
          ...current,
          workspaceEditors,
          mutedWorkspaceSections,
          ignoredWorkspaceIssues,
          ignoredWorkspaceCandidates,
          pinnedWorkspaceIds: current.pinnedWorkspaceIds.filter(
            (id) => id !== workspaceId,
          ),
          recentWorkspaceIds,
          lastWorkspaceId:
            current.lastWorkspaceId === workspaceId
              ? recentWorkspaceIds[0]
              : current.lastWorkspaceId,
        };
      });

      void clearProcessHistory(workspaceId).catch((error) => {
        setProcessActionError(
          `Workspace removed, but local log cleanup failed · ${error instanceof Error ? error.message : String(error)}`,
        );
      });
    },
    [commitPreferences],
  );

  return {
    workspaces,
    editors,
    settings,
    preferencesLoaded,
    lastWorkspaceId: preferences.lastWorkspaceId,
    pinnedWorkspaceIds: preferences.pinnedWorkspaceIds,
    recentWorkspaceIds: preferences.recentWorkspaceIds,
    workspaceEditors: preferences.workspaceEditors,
    mutedWorkspaceSections: preferences.mutedWorkspaceSections,
    ignoredWorkspaceIssues: preferences.ignoredWorkspaceIssues,
    ignoredWorkspaceCandidates: preferences.ignoredWorkspaceCandidates,
    workspaceLoadError,
    processActionError,
    hydrateWorkspace,
    refreshWorkspacePorts,
    refreshWorkspaceUsage,
    startComponent,
    stopComponent,
    restartComponent,
    runAll,
    stopAll,
    loadLogHistory,
    clearLogs,
    trustWorkspace,
    acceptDetectedWorkspace,
    refreshGit,
    loadWorkspaceBranches,
    changeGitBranch,
    updateSettings,
    refreshEditors,
    chooseCustomEditor,
    setWorkspaceEditor,
    recordWorkspaceSelection,
    togglePinnedWorkspace,
    toggleWorkspaceSectionMuted,
    ignoreWorkspaceIssue,
    restoreWorkspaceIssues,
    openWorkspaceInIde,
    removeWorkspace,
  };
}
