import { describe, expect, it } from "vitest";
import {
  dedupeEditors,
  editorIdentityKey,
  sameEditorIdentity,
} from "./editor-utils";
import type { ComponentStatus, Workspace, WorkspaceComponent } from "./model";
import {
  workspaceAttentionReasons,
  workspaceNeedsAttention,
} from "./workspace-attention";
import { workspaceRunStatus } from "./workspace-run-status";

function component(
  id: string,
  status: ComponentStatus = "stopped",
): WorkspaceComponent {
  return {
    id,
    name: id,
    category: "Service",
    technology: "Node.js",
    runtime: "Node.js",
    workingDirectory: ".",
    command: "npm run dev",
    status,
    includeInRunAll: true,
  };
}

function workspace(
  components: WorkspaceComponent[] = [component("api")],
): Workspace {
  return {
    id: "workspace",
    name: "Workspace",
    path: "/tmp/workspace",
    branch: "main",
    ide: "Visual Studio Code",
    trusted: true,
    executionAvailable: true,
    health: "healthy",
    components,
    issues: [],
    activity: [],
    logs: [],
    environmentFiles: [],
    environmentVariables: [],
    git: {
      state: "ready",
      branch: "main",
      ahead: 0,
      behind: 0,
      changes: [],
      commits: [],
    },
  };
}

describe("editor identity", () => {
  it("treats VS Code aliases as the same product", () => {
    expect(editorIdentityKey({ label: "VS Code" })).toBe("visual studio code");
    expect(editorIdentityKey({ label: "Visual Studio Code" })).toBe(
      "visual studio code",
    );
    expect(editorIdentityKey({ label: "code" })).toBe("visual studio code");
    expect(
      sameEditorIdentity(
        { label: "VS Code", path: "/Applications/Visual Studio Code.app" },
        { label: "code", path: "/usr/local/bin/code" },
      ),
    ).toBe(true);
  });

  it("deduplicates one product while preserving the preferred selection", () => {
    const preferred = {
      label: "Visual Studio Code",
      path: "/Applications/Visual Studio Code.app",
    };
    const editors = dedupeEditors(
      [
        { label: "VS Code", path: "/usr/local/bin/code" },
        { label: "Cursor", path: "/Applications/Cursor.app" },
      ],
      [preferred],
    );

    expect(editors).toHaveLength(2);
    expect(editors).toContainEqual(preferred);
    expect(editors.filter((editor) => editorIdentityKey(editor) === "visual studio code"))
      .toHaveLength(1);
  });
});

describe("workspace run status", () => {
  it("reports idle when no component is active", () => {
    expect(workspaceRunStatus(workspace()).kind).toBe("idle");
  });

  it("reports fully running only when every component is DECC-owned and active", () => {
    const status = workspaceRunStatus(
      workspace([component("api", "running"), component("web", "running")]),
    );

    expect(status.kind).toBe("running");
    expect(status.activeCount).toBe(2);
    expect(status.shortLabel).toBe("Running");
  });

  it("reports a ratio for a partially running multi-component workspace", () => {
    const status = workspaceRunStatus(
      workspace([
        component("api", "running"),
        component("web", "stopped"),
        component("worker", "stopped"),
      ]),
    );

    expect(status.kind).toBe("partial");
    expect(status.activeCount).toBe(1);
    expect(status.shortLabel).toBe("1/3 running");
  });

  it("distinguishes external activity from DECC-owned execution", () => {
    const status = workspaceRunStatus(
      workspace([
        component("api", "running"),
        component("web", "external"),
        component("worker", "stopped"),
      ]),
    );

    expect(status.kind).toBe("partial");
    expect(status.ownedActive).toBe(1);
    expect(status.external).toBe(1);
    expect(status.shortLabel).toBe("2/3 active");
    expect(status.detail).toContain("1 under DECC and 1 external");
  });

  it("prioritizes transition state over a generic partial label", () => {
    const status = workspaceRunStatus(
      workspace([
        component("api", "starting"),
        component("web", "stopped"),
      ]),
    );

    expect(status.kind).toBe("starting");
    expect(status.shortLabel).toBe("1/2 active");
  });
});

describe("workspace attention", () => {
  it("hides ignored issues from attention state", () => {
    const value = workspace();
    value.health = "attention";
    value.issues = [
      {
        id: "runtime:api:node",
        title: "Node.js needs attention",
        detail: "Installed runtime is incompatible.",
        tone: "warning",
      },
    ];

    expect(workspaceNeedsAttention(value)).toBe(true);
    expect(
      workspaceNeedsAttention(value, [], ["runtime:api:node"]),
    ).toBe(false);
  });

  it("mutes environment issues and missing-variable reasons together", () => {
    const value = workspace();
    value.health = "attention";
    value.issues = [
      {
        id: "environment:inspection-error",
        title: "Environment inspection failed",
        detail: "Could not read .env.",
        tone: "warning",
      },
    ];
    value.environmentVariables = [
      { name: "DATABASE_URL", status: "missing" },
    ];

    expect(workspaceAttentionReasons(value)).toHaveLength(2);
    expect(workspaceAttentionReasons(value, ["environment"])).toEqual([]);
  });

  it("summarizes missing variables without exposing values", () => {
    const value = workspace();
    value.environmentVariables = [
      { name: "A", status: "missing" },
      { name: "B", status: "missing" },
      { name: "C", status: "missing" },
      { name: "D", status: "missing" },
      { name: "E", status: "missing" },
    ];

    const reason = workspaceAttentionReasons(value)[0];
    expect(reason.title).toBe("5 environment variables missing");
    expect(reason.detail).toBe("A, B, C, D and 1 more");
  });
});
