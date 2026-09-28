import type { Workspace, WorkspaceSection } from "./model";

export type WorkspaceAttentionReason = {
  id: string;
  title: string;
  detail?: string;
  source: "issue" | "environment";
};

export function workspaceAttentionReasons(
  workspace: Workspace,
  mutedSections: WorkspaceSection[] = [],
  ignoredIssueIds: string[] = [],
): WorkspaceAttentionReason[] {
  const muted = new Set(mutedSections);
  const ignored = new Set(ignoredIssueIds);

  const issueReasons = workspace.issues
    .filter((issue) => !ignored.has(issue.id))
    .filter(
      (issue) =>
        !(muted.has("environment") && issue.id.startsWith("environment:")),
    )
    .map((issue) => ({
      id: issue.id,
      title: issue.title,
      detail: issue.detail,
      source: "issue" as const,
    }));

  if (muted.has("environment")) return issueReasons;

  const missing = workspace.environmentVariables.filter(
    (variable) => variable.status === "missing",
  );
  if (missing.length === 0) return issueReasons;

  const names = missing
    .slice(0, 4)
    .map((variable) => variable.name)
    .join(", ");
  const remaining = missing.length - Math.min(missing.length, 4);

  return [
    ...issueReasons,
    {
      id: "environment:missing-variables",
      title: `${missing.length} environment variable${missing.length === 1 ? "" : "s"} missing`,
      detail:
        remaining > 0
          ? `${names} and ${remaining} more`
          : names,
      source: "environment" as const,
    },
  ];
}

export function workspaceNeedsAttention(
  workspace: Workspace,
  mutedSections: WorkspaceSection[] = [],
  ignoredIssueIds: string[] = [],
): boolean {
  return workspaceAttentionReasons(
    workspace,
    mutedSections,
    ignoredIssueIds,
  ).length > 0;
}
