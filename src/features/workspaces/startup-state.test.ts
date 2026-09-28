import { describe, expect, it } from "vitest";
import { defaultPreferences } from "./defaults";
import {
  reconcileStartupPreferences,
  resolveInitialWorkspaceId,
} from "./startup-state";

describe("startup preference reconciliation", () => {
  it("drops references to workspaces that are no longer persisted", () => {
    const preferences = {
      ...defaultPreferences,
      lastWorkspaceId: "gone",
      pinnedWorkspaceIds: ["kept", "gone"],
      recentWorkspaceIds: ["gone", "kept"],
      workspaceEditors: {
        kept: {
          label: "Cursor",
          path: "/Applications/Cursor.app",
        },
        gone: {
          label: "Xcode",
          path: "/Applications/Xcode.app",
        },
      },
      mutedWorkspaceSections: {
        kept: ["logs" as const],
        gone: ["git" as const],
      },
      ignoredWorkspaceIssues: {
        gone: ["runtime:api:node"],
      },
      ignoredWorkspaceCandidates: {
        gone: ["worker"],
      },
    };

    const result = reconcileStartupPreferences(preferences, ["kept"]);

    expect(result.changed).toBe(true);
    expect(result.preferences.lastWorkspaceId).toBeUndefined();
    expect(result.preferences.pinnedWorkspaceIds).toEqual(["kept"]);
    expect(result.preferences.recentWorkspaceIds).toEqual(["kept"]);
    expect(Object.keys(result.preferences.workspaceEditors)).toEqual(["kept"]);
    expect(Object.keys(result.preferences.mutedWorkspaceSections)).toEqual([
      "kept",
    ]);
    expect(result.preferences.ignoredWorkspaceIssues).toEqual({});
    expect(result.preferences.ignoredWorkspaceCandidates).toEqual({});
  });

  it("leaves already consistent preferences untouched", () => {
    const preferences = {
      ...defaultPreferences,
      lastWorkspaceId: "kept",
      pinnedWorkspaceIds: ["kept"],
      recentWorkspaceIds: ["kept"],
    };

    const result = reconcileStartupPreferences(preferences, ["kept"]);

    expect(result.changed).toBe(false);
    expect(result.preferences).toEqual(preferences);
  });
});

describe("initial workspace restoration", () => {
  it("restores the last workspace only when enabled and still available", () => {
    expect(
      resolveInitialWorkspaceId({
        workspaceIds: ["first", "last"],
        restoreLastWorkspace: true,
        lastWorkspaceId: "last",
      }),
    ).toBe("last");
  });

  it("falls back deterministically to the first workspace for stale state", () => {
    expect(
      resolveInitialWorkspaceId({
        workspaceIds: ["first", "second"],
        restoreLastWorkspace: true,
        lastWorkspaceId: "gone",
      }),
    ).toBe("first");
  });

  it("does not restore last workspace when the preference is disabled", () => {
    expect(
      resolveInitialWorkspaceId({
        workspaceIds: ["first", "last"],
        restoreLastWorkspace: false,
        lastWorkspaceId: "last",
      }),
    ).toBe("first");
  });

  it("returns null when no workspace exists", () => {
    expect(
      resolveInitialWorkspaceId({
        workspaceIds: [],
        restoreLastWorkspace: true,
        lastWorkspaceId: "gone",
      }),
    ).toBeNull();
  });
});
