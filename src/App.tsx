import { useEffect, useMemo, useRef, useState } from "react";
import {
  FolderPlus,
  Logs,
  Play,
  Settings,
  Square,
  TerminalSquare,
} from "lucide-react";
import { AppSidebar } from "@/components/layout/app-sidebar";
import { TopBar } from "@/components/layout/top-bar";
import { AppToast } from "@/components/shared/app-toast";
import { ConfirmDialog } from "@/components/shared/confirm-dialog";
import {
  CommandPalette,
  type PaletteAction,
} from "@/features/command-palette/command-palette";
import { SettingsPage } from "@/features/settings/settings-page";
import {
  cancelWorkspaceScan as cancelNativeWorkspaceScan,
  chooseWorkspaceFolder,
  scanWorkspace,
} from "@/services/desktop";
import type {
  DetectedCandidate,
  WorkspaceSection,
} from "@/features/workspaces/model";
import { resolveInitialWorkspaceId } from "@/features/workspaces/startup-state";
import { useDecc } from "@/features/workspaces/use-decc";
import { WorkspaceScreen } from "@/features/workspaces/workspace-screen";
import {
  DetectionReview,
  WorkspaceEmpty,
  WorkspaceScanning,
} from "@/features/workspaces/workspace-setup";
import "./App.css";

type AppMode = "workspace" | "scan" | "review" | "settings";

function App() {
  const {
    workspaces,
    editors,
    settings,
    preferencesLoaded,
    lastWorkspaceId,
    pinnedWorkspaceIds,
    recentWorkspaceIds,
    workspaceEditors,
    mutedWorkspaceSections,
    ignoredWorkspaceIssues,
    ignoredWorkspaceCandidates,
    workspaceLoadError,
    processActionError,
    hydrateWorkspace,
    refreshWorkspacePorts,
    refreshWorkspaceUsage,
    startComponent,
    stopComponent,
    restartComponent,
    runAll,
    stopAll,
    loadLogHistory,
    clearLogs,
    trustWorkspace,
    acceptDetectedWorkspace,
    refreshGit,
    loadWorkspaceBranches,
    changeGitBranch,
    updateSettings,
    refreshEditors,
    chooseCustomEditor,
    setWorkspaceEditor,
    recordWorkspaceSelection,
    togglePinnedWorkspace,
    toggleWorkspaceSectionMuted,
    ignoreWorkspaceIssue,
    restoreWorkspaceIssues,
    openWorkspaceInIde,
    removeWorkspace,
  } = useDecc();

  const [mode, setMode] = useState<AppMode>("workspace");
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [activeWorkspaceId, setActiveWorkspaceId] = useState<string | null>(null);
  const restoredWorkspaceRef = useRef(false);
  const [activeSection, setActiveSection] = useState<WorkspaceSection>("overview");
  const [candidates, setCandidates] = useState<DetectedCandidate[]>([]);
  const [selectedWorkspacePath, setSelectedWorkspacePath] = useState("");
  const [acceptingWorkspace, setAcceptingWorkspace] = useState(false);
  const [rescanWorkspaceId, setRescanWorkspaceId] = useState<string | null>(null);
  const scanRequestRef = useRef(0);
  const activeScanIdRef = useRef<string | null>(null);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [toast, setToast] = useState<string>();
  const [confirmStopWorkspaceId, setConfirmStopWorkspaceId] = useState<string | null>(null);
  const [confirmRemoveWorkspaceId, setConfirmRemoveWorkspaceId] = useState<string | null>(null);

  const activeWorkspace = useMemo(
    () => workspaces.find((workspace) => workspace.id === activeWorkspaceId),
    [activeWorkspaceId, workspaces],
  );

  useEffect(() => {
    if (!preferencesLoaded || restoredWorkspaceRef.current) return;

    const restoredId = resolveInitialWorkspaceId({
      workspaceIds: workspaces.map((workspace) => workspace.id),
      restoreLastWorkspace: settings.restoreLastWorkspace,
      lastWorkspaceId,
    });

    restoredWorkspaceRef.current = true;
    setActiveWorkspaceId(restoredId);
    if (!restoredId) setMode("workspace");
  }, [
    lastWorkspaceId,
    preferencesLoaded,
    settings.restoreLastWorkspace,
    workspaces,
  ]);

  useEffect(() => {
    const workspaceId = activeWorkspace?.id;
    if (mode !== "workspace" || !workspaceId) return;

    void Promise.all([
      hydrateWorkspace(workspaceId),
      refreshGit(workspaceId),
      refreshWorkspaceUsage(workspaceId),
    ]);
  }, [
    activeWorkspace?.id,
    hydrateWorkspace,
    mode,
    refreshGit,
    refreshWorkspaceUsage,
  ]);

  useEffect(() => {
    const workspaceId = activeWorkspace?.id;
    if (mode !== "workspace" || !workspaceId) return;

    let inFlight = false;
    const refresh = () => {
      if (inFlight || document.visibilityState === "hidden") return;
      inFlight = true;
      void Promise.all([
        refreshWorkspacePorts(workspaceId),
        refreshWorkspaceUsage(workspaceId),
      ]).finally(() => {
        inFlight = false;
      });
    };

    const timer = window.setInterval(refresh, 4000);
    return () => window.clearInterval(timer);
  }, [
    activeWorkspace?.id,
    mode,
    refreshWorkspacePorts,
    refreshWorkspaceUsage,
  ]);

  useEffect(() => {
    const root = document.documentElement;
    root.dataset.themeMode = settings.theme;
    const prefersDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
    root.classList.toggle(
      "dark",
      settings.theme === "dark" || (settings.theme === "system" && prefersDark),
    );
  }, [settings.theme]);

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      const command = event.metaKey || event.ctrlKey;

      if (command && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setPaletteOpen((open) => !open);
      }
      if (command && event.key === "1") {
        event.preventDefault();
        setMode("workspace");
        setActiveSection("overview");
      }
      if (command && event.key === "3") {
        event.preventDefault();
        setMode("workspace");
        setActiveSection("logs");
      }
      if (command && event.key === "4") {
        event.preventDefault();
        setMode("workspace");
        setActiveSection("git");
      }
      if (command && event.key === ",") {
        event.preventDefault();
        setMode("settings");
      }
      if (event.key === "Escape") setPaletteOpen(false);
    }

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  useEffect(() => {
    if (!toast) return;

    const timer = window.setTimeout(() => setToast(undefined), 2200);
    return () => window.clearTimeout(timer);
  }, [toast]);

  useEffect(() => {
    if (workspaceLoadError) {
      setToast(`Could not load saved workspaces · ${workspaceLoadError}`);
    }
  }, [workspaceLoadError]);

  useEffect(() => {
    if (processActionError) {
      setToast(processActionError);
    }
  }, [processActionError]);

  const notify = (message: string) => setToast(message);

  const scanWorkspacePath = async (
    path: string,
    sourceWorkspaceId: string | null,
  ) => {
    const requestId = scanRequestRef.current + 1;
    const scanId = crypto.randomUUID();
    const previousScanId = activeScanIdRef.current;
    scanRequestRef.current = requestId;
    activeScanIdRef.current = scanId;
    if (previousScanId) {
      void cancelNativeWorkspaceScan(previousScanId).catch(() => undefined);
    }

    setRescanWorkspaceId(sourceWorkspaceId);
    setSelectedWorkspacePath(path);
    setCandidates([]);
    setMode("scan");

    try {
      const result = await scanWorkspace(path, scanId);
      if (scanRequestRef.current !== requestId) return;

      setSelectedWorkspacePath(result.rootPath);
      const sourceWorkspace = sourceWorkspaceId
        ? workspaces.find((workspace) => workspace.id === sourceWorkspaceId)
        : undefined;
      const existingComponentIds = new Set(
        sourceWorkspace?.components.map((component) => component.id) ?? [],
      );
      const ignoredCandidateIds = new Set(
        sourceWorkspaceId
          ? ignoredWorkspaceCandidates[sourceWorkspaceId] ?? []
          : [],
      );
      const reviewedCandidates = result.candidates.map((candidate) => ({
        ...candidate,
        included: ignoredCandidateIds.has(candidate.id)
          ? false
          : existingComponentIds.has(candidate.id)
            ? true
            : candidate.included,
      }));
      setCandidates(reviewedCandidates);

      const needsReview =
        Boolean(sourceWorkspaceId) ||
        reviewedCandidates.length === 0 ||
        reviewedCandidates.some((candidate) => candidate.confidence !== "high");
      if (needsReview) {
        setMode("review");
        return;
      }

      const workspaceId = await acceptDetectedWorkspace(
        result.rootPath,
        reviewedCandidates,
      );
      if (scanRequestRef.current !== requestId) return;
      setRescanWorkspaceId(null);
      selectWorkspace(workspaceId);
      notify("Workspace added");
    } catch (error) {
      if (scanRequestRef.current !== requestId) return;

      const message = error instanceof Error ? error.message : String(error);
      if (sourceWorkspaceId) {
        setRescanWorkspaceId(null);
        setMode("workspace");
        notify(`Could not rescan workspace · ${message}`);
      } else {
        setMode("workspace");
        notify(`Could not scan workspace · ${message}`);
      }
    } finally {
      if (activeScanIdRef.current === scanId) {
        activeScanIdRef.current = null;
      }
    }
  };

  const chooseAndScanWorkspace = async () => {
    let path: string | null;
    try {
      path = await chooseWorkspaceFolder();
    } catch (error) {
      notify(
        `Could not open folder picker · ${error instanceof Error ? error.message : String(error)}`,
      );
      return;
    }

    if (!path) return;
    await scanWorkspacePath(path, null);
  };

  const rescanWorkspace = async (workspaceId: string) => {
    const workspace = workspaces.find((item) => item.id === workspaceId);
    if (!workspace) return;
    await scanWorkspacePath(workspace.path, workspaceId);
  };

  const cancelWorkspaceScan = () => {
    const scanId = activeScanIdRef.current;
    scanRequestRef.current += 1;
    activeScanIdRef.current = null;
    if (scanId) {
      void cancelNativeWorkspaceScan(scanId).catch(() => undefined);
    }
    setRescanWorkspaceId(null);
    setMode("workspace");
  };

  const persistReviewedWorkspace = async () => {
    if (acceptingWorkspace) return;
    setAcceptingWorkspace(true);
    try {
      const wasRescan = Boolean(rescanWorkspaceId);
      const workspaceId = await acceptDetectedWorkspace(
        selectedWorkspacePath,
        candidates,
      );
      setRescanWorkspaceId(null);
      selectWorkspace(workspaceId);
      notify(
        wasRescan
          ? "Workspace updated from the reviewed rescan"
          : "Workspace saved locally from detected components",
      );
    } catch (error) {
      notify(
        `Could not save workspace · ${error instanceof Error ? error.message : String(error)}`,
      );
    } finally {
      setAcceptingWorkspace(false);
    }
  };

  const trustSelectedWorkspace = async (workspaceId: string) => {
    try {
      await trustWorkspace(workspaceId);
      notify("Workspace trusted for future command execution");
    } catch (error) {
      notify(
        `Could not update workspace trust · ${error instanceof Error ? error.message : String(error)}`,
      );
    }
  };

  const requestStopAll = (workspaceId: string) => {
    if (settings.confirmBeforeStopAll) {
      setConfirmStopWorkspaceId(workspaceId);
      return;
    }
    stopAll(workspaceId);
  };

  const openIdeTarget = async (
    workspaceId: string,
    options: { componentId?: string; relativePath?: string } = {},
  ) => {
    const workspace = workspaces.find((item) => item.id === workspaceId);
    if (!workspace) return;

    try {
      await openWorkspaceInIde(workspaceId, options);
    } catch (error) {
      notify(
        `Could not open ${workspace.ide} · ${error instanceof Error ? error.message : String(error)}`,
      );
    }
  };

  const confirmRemoveWorkspace = async () => {
    const workspaceId = confirmRemoveWorkspaceId;
    if (!workspaceId) return;

    const workspace = workspaces.find((item) => item.id === workspaceId);
    const fallback = workspaces.find((item) => item.id !== workspaceId);
    try {
      await removeWorkspace(workspaceId);
      if (activeWorkspaceId === workspaceId) {
        setActiveWorkspaceId(fallback?.id ?? null);
        if (fallback) recordWorkspaceSelection(fallback.id);
      }
      notify(
        workspace
          ? `${workspace.name} removed from DECC. Project files were not changed.`
          : "Workspace removed from DECC",
      );
    } catch (error) {
      notify(
        `Could not remove workspace · ${error instanceof Error ? error.message : String(error)}`,
      );
    } finally {
      setConfirmRemoveWorkspaceId(null);
    }
  };

  const selectWorkspace = (workspaceId: string) => {
    setActiveWorkspaceId(workspaceId);
    recordWorkspaceSelection(workspaceId);
    setActiveSection("overview");
    setMode("workspace");
  };

  const togglePinWorkspace = (workspaceId: string) => {
    togglePinnedWorkspace(workspaceId);
  };

  const openWorkspaceSection = (section: WorkspaceSection) => {
    setMode("workspace");
    setActiveSection(section);
  };

  const paletteActions: PaletteAction[] = [
    {
      id: "add-workspace",
      label: "Add workspace",
      detail: "Scan another project folder",
      keywords: "project folder scan",
      icon: <FolderPlus />,
      run: () => void chooseAndScanWorkspace(),
    },
    {
      id: "settings",
      label: "Open Settings",
      detail: "Editor, appearance, logs, and safety",
      keywords: "preferences",
      icon: <Settings />,
      shortcut: "⌘,",
      run: () => setMode("settings"),
    },
  ];

  if (activeWorkspace) {
    paletteActions.unshift(
      {
        id: "run-all",
        label: `Run all · ${activeWorkspace.name}`,
        detail: "Start components included in the workspace run plan",
        keywords: "start processes",
        icon: <Play />,
        shortcut: "R",
        run: () => runAll(activeWorkspace.id),
      },
      {
        id: "stop-all",
        label: `Stop all · ${activeWorkspace.name}`,
        detail: "Stop DECC-owned processes; external processes are left alone",
        keywords: "stop processes",
        icon: <Square />,
        shortcut: "⇧R",
        run: () => requestStopAll(activeWorkspace.id),
      },
      {
        id: "logs",
        label: "Open Logs",
        detail: activeWorkspace.name,
        keywords: "stdout stderr terminal",
        icon: <Logs />,
        shortcut: "⌘3",
        run: () => openWorkspaceSection("logs"),
      },
    );
  }

  for (const workspace of workspaces) {
    paletteActions.push({
      id: `workspace-${workspace.id}`,
      label: `Switch to ${workspace.name}`,
      detail: workspace.path,
      keywords: `workspace project ${workspace.branch}`,
      icon: <TerminalSquare />,
      run: () => selectWorkspace(workspace.id),
    });
  }

  const sectionLabel =
    mode === "settings"
      ? "Settings"
      : mode === "review"
        ? "Review"
        : mode === "scan"
          ? "Scanning"
          : activeWorkspace
              ? activeSection.charAt(0).toUpperCase() + activeSection.slice(1)
              : "Workspaces";

  return (
    <div
      className={sidebarCollapsed ? "app-shell app-shell--compact" : "app-shell"}
    >
      <TopBar
        workspace={mode === "workspace" ? activeWorkspace : undefined}
        section={sectionLabel}
        sidebarCollapsed={sidebarCollapsed}
        theme={settings.theme}
        onToggleSidebar={() => setSidebarCollapsed((collapsed) => !collapsed)}
        onThemeChange={(theme) => updateSettings({ theme })}
        onSearch={() => setPaletteOpen(true)}
      />

      <AppSidebar
        workspaces={workspaces}
        activeWorkspaceId={activeWorkspaceId}
        pinnedWorkspaceIds={pinnedWorkspaceIds}
        recentWorkspaceIds={recentWorkspaceIds}
        mutedWorkspaceSections={mutedWorkspaceSections}
        ignoredWorkspaceIssues={ignoredWorkspaceIssues}
        settingsActive={mode === "settings"}
        collapsed={sidebarCollapsed}
        onSelect={selectWorkspace}
        onTogglePin={togglePinWorkspace}
        onExpand={() => setSidebarCollapsed(false)}
        onAdd={() => void chooseAndScanWorkspace()}
        onSettings={() => setMode("settings")}
      />

      <div className="app-surface">
        <main className="app-content">
          {mode === "workspace" &&
            (activeWorkspace ? (
              <WorkspaceScreen
                key={activeWorkspace.id}
                workspace={activeWorkspace}
                section={activeSection}
                onSectionChange={setActiveSection}
                onStartComponent={(componentId) =>
                  startComponent(activeWorkspace.id, componentId)
                }
                onStopComponent={(componentId) =>
                  stopComponent(activeWorkspace.id, componentId)
                }
                onRestartComponent={(componentId) =>
                  restartComponent(activeWorkspace.id, componentId)
                }
                logRetentionDays={settings.logRetentionDays}
                editors={editors}
                defaultEditor={settings.defaultEditor}
                editorOverride={workspaceEditors[activeWorkspace.id]}
                mutedSections={mutedWorkspaceSections[activeWorkspace.id] ?? []}
                ignoredIssueIds={ignoredWorkspaceIssues[activeWorkspace.id] ?? []}
                onEditorChange={(editor) =>
                  setWorkspaceEditor(activeWorkspace.id, editor)
                }
                onChooseEditor={() => {
                  void chooseCustomEditor().then((editor) => {
                    if (editor) setWorkspaceEditor(activeWorkspace.id, editor);
                  });
                }}
                onRefreshEditors={() => void refreshEditors()}
                onRunAll={() => runAll(activeWorkspace.id)}
                onStopAll={() => requestStopAll(activeWorkspace.id)}
                onLoadProcessHistory={loadLogHistory}
                onClearLogs={() => clearLogs(activeWorkspace.id)}
                onTrust={() => void trustSelectedWorkspace(activeWorkspace.id)}
                onRefreshGit={() => refreshGit(activeWorkspace.id)}
                onLoadGitBranches={() =>
                  loadWorkspaceBranches(activeWorkspace.id)
                }
                onSwitchGitBranch={(branch) =>
                  changeGitBranch(activeWorkspace.id, branch)
                }
                onOpenIde={(options) =>
                  void openIdeTarget(activeWorkspace.id, options)
                }
                onRescan={() => void rescanWorkspace(activeWorkspace.id)}
                onRemove={() => setConfirmRemoveWorkspaceId(activeWorkspace.id)}
                onToggleSectionMuted={(section) =>
                  toggleWorkspaceSectionMuted(activeWorkspace.id, section)
                }
                onIgnoreIssue={(issueId) =>
                  ignoreWorkspaceIssue(activeWorkspace.id, issueId)
                }
                onRestoreIssues={() => restoreWorkspaceIssues(activeWorkspace.id)}
                onNotify={notify}
              />
            ) : (
              <WorkspaceEmpty onAdd={() => void chooseAndScanWorkspace()} />
            ))}

          {mode === "scan" && (
            <WorkspaceScanning
              path={selectedWorkspacePath}
              onCancel={cancelWorkspaceScan}
            />
          )}

          {mode === "review" && (
            <DetectionReview
              candidates={candidates}
              workspacePath={selectedWorkspacePath}
              saving={acceptingWorkspace}
              mode={rescanWorkspaceId ? "rescan" : "add"}
              onChange={setCandidates}
              onCancel={() => {
                if (rescanWorkspaceId) {
                  setRescanWorkspaceId(null);
                  setMode("workspace");
                } else {
                  setMode("workspace");
                }
              }}
              onConfirm={() => void persistReviewedWorkspace()}
            />
          )}

          {mode === "settings" && (
            <SettingsPage
              settings={settings}
              editors={editors}
              onChange={updateSettings}
              onEditorChange={(editor) => updateSettings({ defaultEditor: editor })}
              onChooseEditor={() => {
                void chooseCustomEditor().then((editor) => {
                  if (editor) updateSettings({ defaultEditor: editor });
                });
              }}
              onRefreshEditors={() => void refreshEditors()}
            />
          )}
        </main>
      </div>

      <CommandPalette
        open={paletteOpen}
        actions={paletteActions}
        onClose={() => setPaletteOpen(false)}
      />

      <ConfirmDialog
        open={Boolean(confirmRemoveWorkspaceId)}
        title="Remove workspace from DECC?"
        detail="This removes the saved workspace from DECC. Your project folder and repository files are not deleted or modified."
        confirmLabel="Remove workspace"
        onCancel={() => setConfirmRemoveWorkspaceId(null)}
        onConfirm={() => void confirmRemoveWorkspace()}
      />

      <ConfirmDialog
        open={Boolean(confirmStopWorkspaceId)}
        title="Stop all DECC-owned processes?"
        detail="External processes will be left untouched. DECC will stop only processes it started and currently owns."
        confirmLabel="Stop all"
        onCancel={() => setConfirmStopWorkspaceId(null)}
        onConfirm={() => {
          if (confirmStopWorkspaceId) {
            stopAll(confirmStopWorkspaceId);
            notify("Stopping DECC-owned processes");
          }
          setConfirmStopWorkspaceId(null);
        }}
      />

      <AppToast message={toast} />
    </div>
  );
}

export default App;
