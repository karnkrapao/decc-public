import type { Workspace } from "./model";

export type WorkspaceRunStatus = {
  active: boolean;
  activeCount: number;
  ownedActive: number;
  external: number;
  total: number;
  kind: "idle" | "running" | "partial" | "starting" | "stopping" | "external";
  shortLabel: string;
  detail: string;
};

export function workspaceRunStatus(workspace: Workspace): WorkspaceRunStatus {
  const total = workspace.components.length;
  const running = workspace.components.filter((component) =>
    ["running", "warning"].includes(component.status),
  ).length;
  const starting = workspace.components.filter(
    (component) => component.status === "starting",
  ).length;
  const stopping = workspace.components.filter(
    (component) => component.status === "stopping",
  ).length;
  const external = workspace.components.filter(
    (component) => component.status === "external",
  ).length;
  const ownedActive = running + starting + stopping;
  const activeCount = ownedActive + external;

  if (activeCount === 0) {
    return {
      active: false,
      activeCount: 0,
      ownedActive: 0,
      external: 0,
      total,
      kind: "idle",
      shortLabel: "Idle",
      detail: "No workspace components are running.",
    };
  }

  if (ownedActive === 0) {
    return {
      active: true,
      activeCount,
      ownedActive,
      external,
      total,
      kind: "external",
      shortLabel: total > 1 ? `${external}/${total} external` : "External",
      detail: `${external} of ${total} component${total === 1 ? "" : "s"} are running outside DECC.`,
    };
  }

  if (stopping > 0) {
    return {
      active: true,
      activeCount,
      ownedActive,
      external,
      total,
      kind: "stopping",
      shortLabel: total > 1 ? `${activeCount}/${total} active` : "Stopping",
      detail: `${activeCount} of ${total} component${total === 1 ? "" : "s"} are active; ${stopping} stopping${external ? `, ${external} external` : ""}.`,
    };
  }

  if (starting > 0) {
    return {
      active: true,
      activeCount,
      ownedActive,
      external,
      total,
      kind: "starting",
      shortLabel: total > 1 ? `${activeCount}/${total} active` : "Starting",
      detail: `${activeCount} of ${total} component${total === 1 ? "" : "s"} are active; ${starting} starting${external ? `, ${external} external` : ""}.`,
    };
  }

  const allOwned = total > 0 && ownedActive === total && external === 0;
  if (allOwned) {
    return {
      active: true,
      activeCount,
      ownedActive,
      external,
      total,
      kind: "running",
      shortLabel: "Running",
      detail: `All ${total} component${total === 1 ? "" : "s"} are running under DECC.`,
    };
  }

  const mixedOwnership = ownedActive > 0 && external > 0;
  return {
    active: true,
    activeCount,
    ownedActive,
    external,
    total,
    kind: "partial",
    shortLabel:
      total > 1
        ? mixedOwnership
          ? `${activeCount}/${total} active`
          : `${ownedActive}/${total} running`
        : "Running",
    detail: mixedOwnership
      ? `${activeCount} of ${total} component${total === 1 ? "" : "s"} are active: ${ownedActive} under DECC and ${external} external.`
      : `${ownedActive} of ${total} component${total === 1 ? "" : "s"} are running under DECC.`,
  };
}
