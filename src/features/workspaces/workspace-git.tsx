import { useState } from "react";
import {
  ArrowDown,
  ArrowUp,
  Bell,
  BellOff,
  CheckCircle2,
  FileDiff,
  GitBranch,
  GitCommitHorizontal,
  RefreshCw,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import type { Workspace } from "./model";

const changeLabel = {
  modified: "M",
  added: "A",
  deleted: "D",
  untracked: "?",
} as const;

const CHANGE_PREVIEW_LIMIT = 8;
const COMMIT_PREVIEW_LIMIT = 6;

export function WorkspaceGit({
  workspace,
  notificationMuted,
  onToggleNotification,
  onRefresh,
}: {
  workspace: Workspace;
  notificationMuted: boolean;
  onToggleNotification: () => void;
  onRefresh: () => Promise<void>;
}) {
  const git = workspace.git;
  const [refreshing, setRefreshing] = useState(false);
  const [showAllChanges, setShowAllChanges] = useState(false);
  const [showAllCommits, setShowAllCommits] = useState(false);
  const nativeState = git.state;
  const state = nativeState ?? "ready";
  const visibleChanges = showAllChanges
    ? git.changes
    : git.changes.slice(0, CHANGE_PREVIEW_LIMIT);
  const visibleCommits = showAllCommits
    ? git.commits
    : git.commits.slice(0, COMMIT_PREVIEW_LIMIT);

  const refresh = async () => {
    if (refreshing) return;
    setRefreshing(true);
    try {
      await onRefresh();
    } finally {
      setRefreshing(false);
    }
  };

  const unavailableMessage =
    state === "notRepository"
      ? "This workspace is not inside a Git working tree."
      : state === "unavailable"
        ? git.error ?? "Git is not available to DECC on its current PATH."
        : state === "error"
          ? git.error ?? "Git context could not be inspected."
          : state === "notInspected"
            ? "Waiting for Git inspection. DECC inspects Git only when this workspace is opened."
            : undefined;

  return (
    <section className="workspace-section-page">
      <div className="section-page-heading">
        <div>
          <h2>Git</h2>
          <p>Workspace source status for context, not a full Git client.</p>
        </div>
        {nativeState ? (
          <div className="section-page-actions">
            <Button
              size="sm"
              variant="outline"
              onClick={onToggleNotification}
              title={
                notificationMuted
                  ? "Show the Git tab badge again"
                  : "Hide the Git change badge for this workspace"
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
              variant="outline"
              onClick={() => void refresh()}
              disabled={refreshing}
            >
              <RefreshCw data-icon="inline-start" />
              {refreshing ? "Refreshing…" : "Refresh"}
            </Button>
          </div>
        ) : null}
      </div>

      {state !== "ready" ? (
        <div className="simple-list">
          <div className="mini-empty">{unavailableMessage}</div>
        </div>
      ) : (
        <>
          <div className="git-summary">
            <div>
              <span className="git-summary__icon">
                <GitBranch />
              </span>
              <span>
                <small>Branch</small>
                <strong>{git.branch}</strong>
              </span>
            </div>
            <div>
              <span className="git-summary__icon">
                <ArrowUp />
              </span>
              <span>
                <small>Ahead</small>
                <strong>{git.ahead}</strong>
              </span>
            </div>
            <div>
              <span className="git-summary__icon">
                <ArrowDown />
              </span>
              <span>
                <small>Behind</small>
                <strong>{git.behind}</strong>
              </span>
            </div>
            <div>
              <span className="git-summary__icon">
                {git.changes.length === 0 ? <CheckCircle2 /> : <FileDiff />}
              </span>
              <span>
                <small>Working tree</small>
                <strong>
                  {git.changes.length === 0
                    ? "Clean"
                    : `${git.changes.length}${git.changesTruncated ? "+" : ""} changes`}
                </strong>
              </span>
            </div>
          </div>

          <div className="git-grid">
            <section>
              <div className="subsection-title">
                <div>
                  <h3>Changes</h3>
                  <p>
                    {git.upstream
                      ? `Tracking ${git.upstream}`
                      : git.repositoryRoot
                        ? `Repository ${git.repositoryRoot}`
                        : "No upstream configured"}
                  </p>
                </div>
              </div>
              <div className="simple-list">
                {visibleChanges.length > 0 ? (
                  visibleChanges.map((change) => (
                    <div className="git-change-row" key={`${change.status}:${change.path}`}>
                      <span className={`git-change git-change--${change.status}`}>
                        {changeLabel[change.status]}
                      </span>
                      <code>{change.path}</code>
                      <span>{change.status}</span>
                    </div>
                  ))
                ) : (
                  <div className="mini-empty">
                    Working tree clean. Nothing to review.
                  </div>
                )}
                {git.changes.length > CHANGE_PREVIEW_LIMIT ? (
                  <button
                    className="list-disclosure"
                    onClick={() => setShowAllChanges((show) => !show)}
                    type="button"
                  >
                    {showAllChanges
                      ? "Show less"
                      : `Show all ${git.changes.length}${git.changesTruncated ? "+" : ""} changes`}
                  </button>
                ) : null}
              </div>
            </section>

            <section>
              <div className="subsection-title">
                <div>
                  <h3>Recent commits</h3>
                  <p>Local history affecting this workspace path.</p>
                </div>
              </div>
              <div className="simple-list">
                {visibleCommits.length > 0 ? (
                  visibleCommits.map((commit) => (
                    <div className="commit-row" key={commit.hash}>
                      <span className="simple-row__icon">
                        <GitCommitHorizontal />
                      </span>
                      <span className="simple-row__main">
                        <strong>{commit.message}</strong>
                        <small>
                          {commit.hash} · {commit.author}
                        </small>
                      </span>
                      <span>{commit.relativeTime}</span>
                    </div>
                  ))
                ) : (
                  <div className="mini-empty">No Git history detected.</div>
                )}
                {git.commits.length > COMMIT_PREVIEW_LIMIT ? (
                  <button
                    className="list-disclosure"
                    onClick={() => setShowAllCommits((show) => !show)}
                    type="button"
                  >
                    {showAllCommits
                      ? "Show less"
                      : `Show all ${git.commits.length} commits`}
                  </button>
                ) : null}
              </div>
            </section>
          </div>
        </>
      )}
    </section>
  );
}
