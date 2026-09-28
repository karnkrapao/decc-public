import type { AppPreferences } from "./model";

function keepWorkspaceKeys<T>(
  values: Record<string, T>,
  workspaceIds: Set<string>,
): Record<string, T> {
  return Object.fromEntries(
    Object.entries(values).filter(([workspaceId]) =>
      workspaceIds.has(workspaceId),
    ),
  );
}

export function reconcileStartupPreferences(
  preferences: AppPreferences,
  availableWorkspaceIds: Iterable<string>,
): { preferences: AppPreferences; changed: boolean } {
  const workspaceIds = new Set(availableWorkspaceIds);
  const next: AppPreferences = {
    ...preferences,
    lastWorkspaceId:
      preferences.lastWorkspaceId &&
      workspaceIds.has(preferences.lastWorkspaceId)
        ? preferences.lastWorkspaceId
        : undefined,
    pinnedWorkspaceIds: preferences.pinnedWorkspaceIds.filter((id) =>
      workspaceIds.has(id),
    ),
    recentWorkspaceIds: preferences.recentWorkspaceIds.filter((id) =>
      workspaceIds.has(id),
    ),
    workspaceEditors: keepWorkspaceKeys(
      preferences.workspaceEditors,
      workspaceIds,
    ),
    mutedWorkspaceSections: keepWorkspaceKeys(
      preferences.mutedWorkspaceSections,
      workspaceIds,
    ),
    ignoredWorkspaceIssues: keepWorkspaceKeys(
      preferences.ignoredWorkspaceIssues,
      workspaceIds,
    ),
    ignoredWorkspaceCandidates: keepWorkspaceKeys(
      preferences.ignoredWorkspaceCandidates,
      workspaceIds,
    ),
  };

  const changed =
    next.lastWorkspaceId !== preferences.lastWorkspaceId ||
    next.pinnedWorkspaceIds.length !== preferences.pinnedWorkspaceIds.length ||
    next.recentWorkspaceIds.length !== preferences.recentWorkspaceIds.length ||
    Object.keys(next.workspaceEditors).length !==
      Object.keys(preferences.workspaceEditors).length ||
    Object.keys(next.mutedWorkspaceSections).length !==
      Object.keys(preferences.mutedWorkspaceSections).length ||
    Object.keys(next.ignoredWorkspaceIssues).length !==
      Object.keys(preferences.ignoredWorkspaceIssues).length ||
    Object.keys(next.ignoredWorkspaceCandidates).length !==
      Object.keys(preferences.ignoredWorkspaceCandidates).length;

  return { preferences: next, changed };
}

export function resolveInitialWorkspaceId({
  workspaceIds,
  restoreLastWorkspace,
  lastWorkspaceId,
}: {
  workspaceIds: string[];
  restoreLastWorkspace: boolean;
  lastWorkspaceId?: string;
}): string | null {
  if (
    restoreLastWorkspace &&
    lastWorkspaceId &&
    workspaceIds.includes(lastWorkspaceId)
  ) {
    return lastWorkspaceId;
  }
  return workspaceIds[0] ?? null;
}
