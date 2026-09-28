import { ArrowLeft, Check, FolderOpen } from "lucide-react";
import { Button } from "@/components/ui/button";
import type { DetectedCandidate } from "./model";

export function WorkspaceEmpty({ onAdd }: { onAdd: () => void }) {
  return (
    <section className="workspace-section-page">
      <div className="system-empty">
        <span className="system-empty__icon">
          <FolderOpen />
        </span>
        <strong>No workspaces yet</strong>
        <p>Choose a project folder to get started.</p>
        <div className="system-empty__actions">
          <Button size="sm" onClick={onAdd}>
            <FolderOpen data-icon="inline-start" />
            Add workspace
          </Button>
        </div>
      </div>
    </section>
  );
}

export function WorkspaceScanning({
  path,
  onCancel,
}: {
  path: string;
  onCancel: () => void;
}) {
  return (
    <div className="flow-page flow-page--scan">
      <div className="scan-state">
        <span className="scan-spinner" aria-hidden="true" />
        <h1>Scanning project…</h1>
        <code className="scan-path">{path}</code>
      </div>

      <button className="flow-cancel" onClick={onCancel} type="button">
        <ArrowLeft />
        Cancel
      </button>
    </div>
  );
}

export function DetectionReview({
  candidates,
  workspacePath,
  saving = false,
  mode = "add",
  onChange,
  onCancel,
  onConfirm,
}: {
  candidates: DetectedCandidate[];
  workspacePath: string;
  saving?: boolean;
  mode?: "add" | "rescan";
  onChange: (candidates: DetectedCandidate[]) => void;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const selectedCount = candidates.filter((candidate) => candidate.included).length;
  const detected = candidates.filter((candidate) => candidate.confidence === "high");
  const possible = candidates.filter((candidate) => candidate.confidence === "possible");

  function toggleCandidate(id: string) {
    onChange(
      candidates.map((candidate) =>
        candidate.id === id
          ? { ...candidate, included: !candidate.included }
          : candidate,
      ),
    );
  }

  return (
    <div className="flow-page flow-page--wide">
      <div className="review-header review-header--compact">
        <div>
          <h1>{mode === "rescan" ? "Review changes" : "Review components"}</h1>
          {possible.length > 0 ? (
            <p>Check the possible matches before saving.</p>
          ) : null}
        </div>

        <div className="review-path">
          <FolderOpen />
          <span>{workspacePath}</span>
        </div>
      </div>

      <div className="candidate-groups">
        {detected.length > 0 ? (
          <CandidateGroup
            title="Detected"
            candidates={detected}
            onToggle={toggleCandidate}
          />
        ) : null}
        {possible.length > 0 ? (
          <CandidateGroup
            title="Possible"
            candidates={possible}
            onToggle={toggleCandidate}
          />
        ) : null}
        {candidates.length === 0 ? (
          <div className="mini-empty">
            No runnable components were detected in this folder.
          </div>
        ) : null}
      </div>

      <div className="review-footer">
        <button className="flow-cancel" onClick={onCancel} type="button">
          <ArrowLeft />
          Back
        </button>

        <div>
          <span>{selectedCount} selected</span>
          <Button disabled={selectedCount === 0 || saving} onClick={onConfirm}>
            {saving
              ? "Saving…"
              : mode === "rescan"
                ? "Update workspace"
                : "Add workspace"}
          </Button>
        </div>
      </div>
    </div>
  );
}

function CandidateGroup({
  title,
  candidates,
  onToggle,
}: {
  title: string;
  candidates: DetectedCandidate[];
  onToggle: (id: string) => void;
}) {
  return (
    <section className="candidate-section">
      <div className="section-heading">
        <h2>{title}</h2>
        <span className="section-count">{candidates.length}</span>
      </div>

      <div className="candidate-list">
        {candidates.map((candidate) => (
          <label className="candidate-row candidate-row--compact" key={candidate.id}>
            <input
              checked={candidate.included}
              onChange={() => onToggle(candidate.id)}
              type="checkbox"
            />
            <span className="candidate-check">
              <Check />
            </span>

            <span className="candidate-main">
              <strong>{candidate.name}</strong>
              <span>{candidate.path}</span>
            </span>

            <span className="candidate-stack">
              {candidate.technology}
              <span>·</span>
              {candidate.runtime}
            </span>

            <code>{candidate.command}</code>
          </label>
        ))}
      </div>
    </section>
  );
}
