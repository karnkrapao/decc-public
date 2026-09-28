import type { AppPreferences } from "./model";

export const defaultPreferences: AppPreferences = {
  workspaceEditors: {},
  mutedWorkspaceSections: {},
  ignoredWorkspaceIssues: {},
  ignoredWorkspaceCandidates: {},
  theme: "system",
  logRetentionDays: 7,
  restoreLastWorkspace: true,
  confirmBeforeStopAll: false,
  pinnedWorkspaceIds: [],
  recentWorkspaceIds: [],
};
