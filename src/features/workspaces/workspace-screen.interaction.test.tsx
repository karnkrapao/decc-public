// @vitest-environment happy-dom

import {
  cleanup,
  fireEvent,
  render,
  screen,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type {
  ComponentStatus,
  Workspace,
  WorkspaceComponent,
} from "./model";
import { WorkspaceScreen } from "./workspace-screen";

const vscode = {
  label: "Visual Studio Code",
  path: "/Applications/Visual Studio Code.app",
};

function component(
  status: ComponentStatus,
  runtimeReady = true,
): WorkspaceComponent {
  return {
    id: "api",
    name: "API",
    category: "Service",
    technology: "Node.js",
    runtime: "Node.js",
    workingDirectory: ".",
    command: "npm run dev",
    status,
    includeInRunAll: true,
    runtimeReady,
  };
}

function workspace(
  status: ComponentStatus,
  runtimeReady = true,
): Workspace {
  return {
    id: "workspace",
    name: "Workspace",
    path: "/tmp/workspace",
    branch: "Checking Git…",
    ide: vscode.label,
    editor: vscode,
    trusted: true,
    executionAvailable: true,
    health: "healthy",
    components: [component(status, runtimeReady)],
    issues: [],
    activity: [],
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

function renderWorkspace(status: ComponentStatus, overrides?: {
  onRunAll?: () => void;
  onStopAll?: () => void;
  onRescan?: () => void;
  onRemove?: () => void;
  runtimeReady?: boolean;
}) {
  const onRunAll = overrides?.onRunAll ?? vi.fn();
  const onStopAll = overrides?.onStopAll ?? vi.fn();
  const onRescan = overrides?.onRescan ?? vi.fn();
  const onRemove = overrides?.onRemove ?? vi.fn();

  const view = render(
    <WorkspaceScreen
      workspace={workspace(status, overrides?.runtimeReady ?? true)}
      section="overview"
      logRetentionDays={7}
      editors={[vscode]}
      defaultEditor={vscode}
      mutedSections={[]}
      ignoredIssueIds={[]}
      onEditorChange={vi.fn()}
      onChooseEditor={vi.fn()}
      onRefreshEditors={vi.fn()}
      onSectionChange={vi.fn()}
      onStartComponent={vi.fn()}
      onStopComponent={vi.fn()}
      onRestartComponent={vi.fn()}
      onRunAll={onRunAll}
      onStopAll={onStopAll}
      onLoadProcessHistory={vi.fn(async () => ({ sessions: [], events: [] }))}
      onClearLogs={vi.fn()}
      onTrust={vi.fn()}
      onRefreshGit={vi.fn(async () => undefined)}
      onLoadGitBranches={vi.fn(async () => [])}
      onSwitchGitBranch={vi.fn(async () => undefined)}
      onOpenIde={vi.fn()}
      onRescan={onRescan}
      onRemove={onRemove}
      onToggleSectionMuted={vi.fn()}
      onIgnoreIssue={vi.fn()}
      onRestoreIssues={vi.fn()}
      onNotify={vi.fn()}
    />,
  );

  return { ...view, onRunAll, onStopAll, onRescan, onRemove };
}

afterEach(cleanup);

describe("WorkspaceScreen header actions", () => {
  it("keeps Run all as the primary enabled action when a component is ready", () => {
    const { onRunAll } = renderWorkspace("stopped");

    const runAll = screen.getByRole("button", { name: "Run all" });
    expect(runAll.hasAttribute("disabled")).toBe(false);

    fireEvent.click(runAll);
    expect(onRunAll).toHaveBeenCalledTimes(1);

    expect(
      screen.getByRole("button", {
        name: "Stop all workspace components",
      }).hasAttribute("disabled"),
    ).toBe(true);
  });

  it("disables Run all when runtime preflight marks the component unavailable", () => {
    const { onRunAll } = renderWorkspace("stopped", { runtimeReady: false });

    const runAll = screen.getByRole("button", { name: "Run all" });
    expect(runAll.hasAttribute("disabled")).toBe(true);

    fireEvent.click(runAll);
    expect(onRunAll).not.toHaveBeenCalled();
  });

  it("enables compact Stop all only while a DECC-owned component is active", () => {
    const { onStopAll } = renderWorkspace("running");

    const stopAll = screen.getByRole("button", {
      name: "Stop all workspace components",
    });
    expect(stopAll.hasAttribute("disabled")).toBe(false);

    fireEvent.click(stopAll);
    expect(onStopAll).toHaveBeenCalledTimes(1);
  });

  it("keeps lifecycle actions in More and closes the menu after Rescan", () => {
    const onRescan = vi.fn();
    const { container } = renderWorkspace("stopped", { onRescan });
    const menu = container.querySelector<HTMLDetailsElement>(
      ".workspace-action-menu",
    );
    expect(menu).not.toBeNull();
    if (!menu) return;

    menu.open = true;
    fireEvent.click(screen.getByRole("button", { name: "Rescan workspace" }));

    expect(onRescan).toHaveBeenCalledTimes(1);
    expect(menu.open).toBe(false);
  });
});
