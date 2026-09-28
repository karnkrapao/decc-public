import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  AppPreferences,
  EditorSelection,
  NativeBulkProcessResult,
  NativeProcessEvent,
  NativeWorkspaceEnvironmentInspection,
  NativeWorkspaceGitHeadInspection,
  NativeWorkspaceGitInspection,
  NativeProcessHistory,
  NativeProcessStatus,
  NativeProcessUsage,
  NativeWorkspacePortInspection,
  NativeWorkspaceRuntimeInspection,
  PersistedWorkspace,
  WorkspaceScanResult,
} from "@/features/workspaces/model";

const PROCESS_EVENT_NAME = "decc-process-event";

export async function chooseWorkspaceFolder(): Promise<string | null> {
  const selection = await open({
    directory: true,
    multiple: false,
    title: "Choose workspace folder",
  });

  if (!selection) return null;
  if (Array.isArray(selection)) return selection[0] ?? null;
  return selection;
}

export async function scanWorkspace(
  path: string,
  scanId: string,
): Promise<WorkspaceScanResult> {
  return invoke<WorkspaceScanResult>("scan_workspace", { path, scanId });
}

export async function cancelWorkspaceScan(scanId: string): Promise<boolean> {
  return invoke<boolean>("cancel_workspace_scan", { scanId });
}

export async function listPersistedWorkspaces(): Promise<PersistedWorkspace[]> {
  return invoke<PersistedWorkspace[]>("list_workspaces");
}

export async function acceptWorkspace(
  rootPath: string,
  candidateIds: string[],
): Promise<PersistedWorkspace> {
  return invoke<PersistedWorkspace>("accept_workspace", {
    rootPath,
    candidateIds,
  });
}

export async function setWorkspaceTrust(
  workspaceId: string,
  trusted: boolean,
): Promise<PersistedWorkspace> {
  return invoke<PersistedWorkspace>("set_workspace_trust", {
    workspaceId,
    trusted,
  });
}

export async function removePersistedWorkspace(
  workspaceId: string,
): Promise<boolean> {
  return invoke<boolean>("remove_workspace", { workspaceId });
}

export async function getPreferences(): Promise<AppPreferences> {
  return invoke<AppPreferences>("get_preferences");
}

export async function savePreferences(
  preferences: AppPreferences,
): Promise<AppPreferences> {
  return invoke<AppPreferences>("save_preferences", { preferences });
}

export async function discoverEditors(): Promise<EditorSelection[]> {
  return invoke<EditorSelection[]>("discover_editors");
}

export async function chooseEditor(): Promise<EditorSelection | null> {
  const selection = await open({
    directory: false,
    multiple: false,
    title: "Choose editor application or executable",
  });
  if (!selection) return null;
  const path = Array.isArray(selection) ? selection[0] : selection;
  if (!path) return null;
  return invoke<EditorSelection>("resolve_editor_path", { path });
}

export async function openInIde(
  workspaceId: string,
  editor: EditorSelection,
  options: { componentId?: string; relativePath?: string } = {},
): Promise<void> {
  return invoke<void>("open_in_ide", {
    workspaceId,
    componentId: options.componentId ?? null,
    relativePath: options.relativePath ?? null,
    editor,
  });
}

export async function startNativeComponent(
  workspaceId: string,
  componentId: string,
): Promise<NativeProcessStatus> {
  return invoke<NativeProcessStatus>("start_component", {
    workspaceId,
    componentId,
  });
}

export async function stopNativeComponent(
  workspaceId: string,
  componentId: string,
): Promise<NativeProcessStatus> {
  return invoke<NativeProcessStatus>("stop_component", {
    workspaceId,
    componentId,
  });
}

export async function restartNativeComponent(
  workspaceId: string,
  componentId: string,
): Promise<NativeProcessStatus> {
  return invoke<NativeProcessStatus>("restart_component", {
    workspaceId,
    componentId,
  });
}

export async function listOwnedProcesses(): Promise<NativeProcessStatus[]> {
  return invoke<NativeProcessStatus[]>("list_owned_processes");
}

export async function inspectProcessUsage(
  workspaceId?: string,
): Promise<NativeProcessUsage[]> {
  return invoke<NativeProcessUsage[]>("inspect_process_usage", {
    workspaceId: workspaceId ?? null,
  });
}

export async function getProcessHistory(
  workspaceId: string,
  maxEvents = 2000,
  sessionId?: string,
): Promise<NativeProcessHistory> {
  return invoke<NativeProcessHistory>("get_process_history", {
    workspaceId,
    maxEvents,
    sessionId: sessionId ?? null,
  });
}

export async function clearProcessHistory(workspaceId: string): Promise<void> {
  return invoke<void>("clear_process_history", { workspaceId });
}

export async function setLogRetentionDays(days: number): Promise<number> {
  return invoke<number>("set_log_retention_days", { days });
}

export async function inspectEnvironment(
  workspaceId?: string,
): Promise<NativeWorkspaceEnvironmentInspection[]> {
  return invoke<NativeWorkspaceEnvironmentInspection[]>("inspect_environment", {
    workspaceId: workspaceId ?? null,
  });
}

export async function inspectGitHeads(
  workspaceId?: string,
): Promise<NativeWorkspaceGitHeadInspection[]> {
  return invoke<NativeWorkspaceGitHeadInspection[]>("inspect_git_heads", {
    workspaceId: workspaceId ?? null,
  });
}

export async function inspectGit(
  workspaceId?: string,
): Promise<NativeWorkspaceGitInspection[]> {
  return invoke<NativeWorkspaceGitInspection[]>("inspect_git", {
    workspaceId: workspaceId ?? null,
  });
}

export async function listGitBranches(workspaceId: string): Promise<string[]> {
  return invoke<string[]>("list_git_branches", { workspaceId });
}

export async function switchGitBranch(
  workspaceId: string,
  branch: string,
): Promise<NativeWorkspaceGitInspection> {
  return invoke<NativeWorkspaceGitInspection>("switch_git_branch", {
    workspaceId,
    branch,
  });
}

export async function inspectPorts(
  workspaceId?: string,
): Promise<NativeWorkspacePortInspection[]> {
  return invoke<NativeWorkspacePortInspection[]>("inspect_ports", {
    workspaceId: workspaceId ?? null,
  });
}

export async function inspectRuntimes(
  workspaceId?: string,
): Promise<NativeWorkspaceRuntimeInspection[]> {
  return invoke<NativeWorkspaceRuntimeInspection[]>("inspect_runtimes", {
    workspaceId: workspaceId ?? null,
  });
}

export async function runNativeWorkspace(
  workspaceId: string,
): Promise<NativeBulkProcessResult> {
  return invoke<NativeBulkProcessResult>("run_all", { workspaceId });
}

export async function stopNativeWorkspace(
  workspaceId: string,
): Promise<NativeBulkProcessResult> {
  return invoke<NativeBulkProcessResult>("stop_all", { workspaceId });
}

export function listenProcessEvents(
  handler: (event: NativeProcessEvent) => void,
): Promise<UnlistenFn> {
  return listen<NativeProcessEvent>(PROCESS_EVENT_NAME, (event) => {
    handler(event.payload);
  });
}
