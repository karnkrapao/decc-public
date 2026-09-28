export type ComponentStatus =
  | "running"
  | "stopped"
  | "external"
  | "warning"
  | "starting"
  | "stopping"
  | "error";

export type WorkspaceSection = "overview" | "logs" | "environment" | "git";

export type WorkspaceComponent = {
  id: string;
  name: string;
  category: string;
  technology: string;
  runtime: string;
  workingDirectory: string;
  command: string;
  status: ComponentStatus;
  includeInRunAll: boolean;
  port?: number;
  expectedPorts?: number[];
  portOrigin?: "owned" | "external";
  portConflict?: boolean;
  portProcessName?: string;
  portProcessCwd?: string;
  pid?: number;
  startedAt?: string;
  cpuPercent?: number;
  memoryBytes?: number;
  lastExitCode?: number;
  runtimeReady?: boolean;
  runtimeBlockingReason?: string;
  runtimeDiagnostics?: RuntimeToolDiagnostic[];
};

export type RuntimeDiagnosticStatus =
  | "ready"
  | "missing"
  | "incompatible"
  | "unknown";

export type RuntimeToolDiagnostic = {
  key: string;
  label: string;
  program: string;
  status: RuntimeDiagnosticStatus;
  installedVersion?: string;
  requiredVersion?: string;
  detail: string;
  blocking: boolean;
};

export type WorkspaceIssue = {
  id: string;
  title: string;
  detail: string;
  tone: "warning" | "info" | "danger";
  componentId?: string;
  action?: "inspect" | "rescan" | "logs";
};

export type WorkspaceActivity = {
  id: string;
  time: string;
  source: string;
  message: string;
  level: "info" | "error" | "success" | "warning";
};

export type WorkspaceLog = {
  id: string;
  time: string;
  componentId: string;
  component: string;
  level: "info" | "success" | "warning" | "error";
  stream: "stdout" | "stderr" | "system";
  message: string;
};

export type EnvironmentFile = {
  name: string;
  path: string;
  kind: "active" | "example" | "override";
  componentId?: string;
  scope?: string;
};

export type EnvironmentVariable = {
  name: string;
  status: "present" | "missing";
  source?: string;
  requiredBy?: string;
  componentId?: string;
};

export type GitChange = {
  path: string;
  status: "modified" | "added" | "deleted" | "untracked";
};

export type GitCommit = {
  hash: string;
  message: string;
  author: string;
  relativeTime: string;
};

export type WorkspaceGit = {
  state?: "notInspected" | "ready" | "notRepository" | "unavailable" | "error";
  repositoryRoot?: string;
  branch: string;
  detached?: boolean;
  upstream?: string;
  ahead: number;
  behind: number;
  changes: GitChange[];
  changesTruncated?: boolean;
  commits: GitCommit[];
  error?: string;
};

export type Workspace = {
  id: string;
  name: string;
  path: string;
  branch: string;
  ide: string;
  editor?: EditorSelection;
  trusted: boolean;
  executionAvailable?: boolean;
  health: "healthy" | "attention";
  components: WorkspaceComponent[];
  issues: WorkspaceIssue[];
  activity: WorkspaceActivity[];
  logs: WorkspaceLog[];
  environmentFiles: EnvironmentFile[];
  environmentVariables: EnvironmentVariable[];
  git: WorkspaceGit;
};

export type DetectedCandidate = {
  id: string;
  name: string;
  path: string;
  technology: string;
  runtime: string;
  command: string;
  expectedPorts?: number[];
  confidence: "high" | "possible";
  included: boolean;
};

export type WorkspaceScanResult = {
  rootPath: string;
  candidates: DetectedCandidate[];
  scannedDirectories: number;
};

export type PersistedComponent = {
  id: string;
  name: string;
  relativePath: string;
  technology: string;
  runtime: string;
  command: string;
  program?: string | null;
  args: string[];
  expectedPorts: number[];
  includeInRunAll: boolean;
  detectionConfidence: "high" | "possible";
};

export type NativeProcessStatus = {
  workspaceId: string;
  componentId: string;
  componentName: string;
  pid: number;
  startedAtMs: number;
};

export type NativeProcessUsage = {
  workspaceId: string;
  componentId: string;
  pid: number;
  cpuPercent?: number;
  memoryBytes?: number;
};

export type NativeProcessEvent = {
  workspaceId: string;
  componentId: string;
  componentName: string;
  kind: "starting" | "started" | "stopping" | "log" | "exited" | "error";
  stream?: "stdout" | "stderr" | "system";
  message: string;
  pid?: number;
  exitCode?: number;
  timestampMs: number;
};

export type NativeRunSessionMetadata = {
  id: string;
  workspaceId: string;
  componentId: string;
  componentName: string;
  pid?: number;
  startedAtMs: number;
  endedAtMs?: number;
  exitCode?: number;
  status: "starting" | "running" | "exited" | "error";
  truncated: boolean;
};

export type NativeProcessHistory = {
  sessions: NativeRunSessionMetadata[];
  events: NativeProcessEvent[];
};

export type NativeProcessActionError = {
  componentId: string;
  message: string;
};

export type NativeBulkProcessResult = {
  processes: NativeProcessStatus[];
  errors: NativeProcessActionError[];
};

export type NativePortListenerObservation = {
  address: string;
  port: number;
  pid?: number;
  processName?: string;
  cwd?: string;
  origin: "owned" | "external";
  matchesExpectedPort: boolean;
};

export type NativeComponentPortInspection = {
  componentId: string;
  expectedPorts: number[];
  listeners: NativePortListenerObservation[];
  externalRunning: boolean;
  hasConflict: boolean;
};

export type NativeWorkspacePortInspection = {
  workspaceId: string;
  components: NativeComponentPortInspection[];
};

export type NativeComponentRuntimeInspection = {
  componentId: string;
  ready: boolean;
  diagnostics: RuntimeToolDiagnostic[];
  blockingMessage?: string;
};

export type NativeWorkspaceRuntimeInspection = {
  workspaceId: string;
  components: NativeComponentRuntimeInspection[];
};

export type NativeEnvironmentFileMetadata = {
  componentId?: string;
  componentName?: string;
  name: string;
  relativePath: string;
  kind: "active" | "example" | "override";
};

export type NativeEnvironmentVariableMetadata = {
  componentId?: string;
  componentName?: string;
  name: string;
  status: "present" | "missing";
  sourcePaths: string[];
  requirementPaths: string[];
};

export type NativeWorkspaceEnvironmentInspection = {
  workspaceId: string;
  files: NativeEnvironmentFileMetadata[];
  variables: NativeEnvironmentVariableMetadata[];
  error?: string;
};

export type NativeWorkspaceGitHeadInspection = {
  workspaceId: string;
  state: "ready" | "notRepository" | "unavailable" | "error";
  branch?: string;
  detached: boolean;
  error?: string;
};

export type NativeWorkspaceGitInspection = {
  workspaceId: string;
  state: "ready" | "notRepository" | "unavailable" | "error";
  repositoryRoot?: string;
  branch?: string;
  detached: boolean;
  upstream?: string;
  ahead: number;
  behind: number;
  changes: GitChange[];
  changesTruncated: boolean;
  commits: GitCommit[];
  error?: string;
};

export type PersistedWorkspace = {
  id: string;
  name: string;
  path: string;
  trusted: boolean;
  components: PersistedComponent[];
  createdAtMs: number;
  updatedAtMs: number;
};

export type EditorSelection = {
  label: string;
  path: string;
};

export type AppSettings = {
  defaultEditor?: EditorSelection;
  theme: "system" | "light" | "dark";
  logRetentionDays: 7 | 14 | 30;
  restoreLastWorkspace: boolean;
  confirmBeforeStopAll: boolean;
};

export type AppPreferences = AppSettings & {
  defaultIde?: string;
  workspaceEditors: Record<string, EditorSelection>;
  mutedWorkspaceSections: Record<string, WorkspaceSection[]>;
  ignoredWorkspaceIssues: Record<string, string[]>;
  ignoredWorkspaceCandidates: Record<string, string[]>;
  lastWorkspaceId?: string;
  pinnedWorkspaceIds: string[];
  recentWorkspaceIds: string[];
};
