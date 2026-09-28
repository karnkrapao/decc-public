import { useState } from "react";
import {
  AlertTriangle,
  Bell,
  BellOff,
  Check,
  FileKey2,
  FileText,
  LockKeyhole,
  X,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import type { Workspace } from "./model";

const FILE_PREVIEW_LIMIT = 6;
const VARIABLE_PREVIEW_LIMIT = 12;

export function WorkspaceEnvironment({
  workspace,
  notificationMuted,
  onToggleNotification,
  onOpenEnv,
}: {
  workspace: Workspace;
  notificationMuted: boolean;
  onToggleNotification: () => void;
  onOpenEnv: () => void;
}) {
  const [showAllFiles, setShowAllFiles] = useState(false);
  const [showAllVariables, setShowAllVariables] = useState(false);
  const missing = workspace.environmentVariables.filter(
    (variable) => variable.status === "missing",
  );
  const visibleFiles = showAllFiles
    ? workspace.environmentFiles
    : workspace.environmentFiles.slice(0, FILE_PREVIEW_LIMIT);
  const visibleVariables = showAllVariables
    ? workspace.environmentVariables
    : workspace.environmentVariables.slice(0, VARIABLE_PREVIEW_LIMIT);

  return (
    <section className="workspace-section-page">
      <div className="section-page-heading">
        <div>
          <h2>Environment</h2>
          <p>
            Variable names and file presence only. Secret values stay hidden.
          </p>
        </div>
        <div className="section-page-actions">
          <Button
            size="sm"
            variant="outline"
            onClick={onToggleNotification}
            title={
              notificationMuted
                ? "Show the Environment warning again"
                : "Hide the Environment warning and tab badge for this workspace"
            }
          >
            {notificationMuted ? (
              <Bell data-icon="inline-start" />
            ) : (
              <BellOff data-icon="inline-start" />
            )}
            {notificationMuted ? "Unmute alert" : "Mute alert"}
          </Button>
          <Button
            size="sm"
            variant="outline"
            disabled={!workspace.editor}
            onClick={onOpenEnv}
            title={!workspace.editor ? "Choose an editor from the workspace header first" : undefined}
          >
            <FileText data-icon="inline-start" />
            {workspace.editor ? `Open .env in ${workspace.ide}` : "Open .env"}
          </Button>
        </div>
      </div>

      {missing.length > 0 && !notificationMuted && (
        <div className="inline-status inline-status--warning">
          <AlertTriangle />
          <div>
            <strong>{missing.length} required variable missing</strong>
            <span>
              Compare your local files with the detected example configuration.
            </span>
          </div>
        </div>
      )}

      <div className="environment-layout">
        <section className="environment-files">
          <div className="subsection-title">
            <div>
              <h3>Files</h3>
              <p>Detected at workspace and component roots.</p>
            </div>
          </div>

          <div className="simple-list">
            {visibleFiles.length > 0 ? (
              visibleFiles.map((file) => (
                <div
                  className="simple-row environment-file-row"
                  key={`${file.componentId ?? "workspace"}:${file.path}`}
                >
                  <span className="simple-row__icon">
                    <FileKey2 />
                  </span>
                  <span className="simple-row__main">
                    <strong title={file.name}>{file.name}</strong>
                    <small title={file.path}>{file.path}</small>
                  </span>
                  <span className="file-kind" title={file.scope ? `${file.scope} · ${file.kind}` : file.kind}>
                    {file.scope ? `${file.scope} · ${file.kind}` : file.kind}
                  </span>
                  <Check className="simple-row__ok" />
                </div>
              ))
            ) : (
              <div className="mini-empty">No environment files detected.</div>
            )}
            {workspace.environmentFiles.length > FILE_PREVIEW_LIMIT ? (
              <button
                className="list-disclosure"
                onClick={() => setShowAllFiles((show) => !show)}
                type="button"
              >
                {showAllFiles
                  ? "Show less"
                  : `Show all ${workspace.environmentFiles.length} files`}
              </button>
            ) : null}
          </div>
        </section>

        <section className="environment-variables">
          <div className="subsection-title">
            <div>
              <h3>Variables</h3>
              <p>Values are intentionally never shown here.</p>
            </div>
            <span className="privacy-note">
              <LockKeyhole />
              Names only
            </span>
          </div>

          <div className="env-table">
            <div className="env-table__head">
              <span>Variable</span>
              <span>Required by</span>
              <span>Source</span>
              <span>Status</span>
            </div>
            {visibleVariables.length > 0 ? (
              visibleVariables.map((variable) => (
                <div
                  className="env-row"
                  key={`${variable.componentId ?? "workspace"}:${variable.name}`}
                >
                  <code title={variable.name}>{variable.name}</code>
                  <span>{variable.requiredBy ?? "Workspace"}</span>
                  <span>{variable.source ?? "—"}</span>
                  <span
                    className={
                      variable.status === "present"
                        ? "env-state env-state--ok"
                        : "env-state env-state--missing"
                    }
                  >
                    {variable.status === "present" ? <Check /> : <X />}
                    {variable.status === "present" ? "Present" : "Missing"}
                  </span>
                </div>
              ))
            ) : (
              <div className="mini-empty">No environment variables inferred.</div>
            )}
            {workspace.environmentVariables.length > VARIABLE_PREVIEW_LIMIT ? (
              <button
                className="list-disclosure"
                onClick={() => setShowAllVariables((show) => !show)}
                type="button"
              >
                {showAllVariables
                  ? "Show less"
                  : `Show all ${workspace.environmentVariables.length} variables`}
              </button>
            ) : null}
          </div>
        </section>
      </div>
    </section>
  );
}
