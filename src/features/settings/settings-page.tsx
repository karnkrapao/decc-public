import {
  Clock3,
  Code2,
  Monitor,
  Moon,
  RotateCcw,
  ShieldCheck,
  Sun,
} from "lucide-react";
import { EditorPicker } from "@/components/shared/editor-picker";
import type { AppSettings, EditorSelection } from "@/features/workspaces/model";

export function SettingsPage({
  settings,
  editors,
  onChange,
  onEditorChange,
  onChooseEditor,
  onRefreshEditors,
}: {
  settings: AppSettings;
  editors: EditorSelection[];
  onChange: (patch: Partial<AppSettings>) => void;
  onEditorChange: (editor: EditorSelection) => void;
  onChooseEditor: () => void;
  onRefreshEditors: () => void;
}) {
  return (
    <div className="settings-page">
      <div className="settings-heading">
        <span className="eyebrow">Preferences</span>
        <h1>Settings</h1>
        <p>Local app preferences for the DECC desktop experience.</p>
      </div>

      <div className="settings-sections">
        <SettingsSection
          icon={<Code2 />}
          title="Default editor"
          description="Detected from this computer. Workspaces can override it individually."
        >
          <EditorPicker
            editors={editors}
            value={settings.defaultEditor}
            onSelect={(editor) => {
              if (editor) onEditorChange(editor);
            }}
            onChoose={onChooseEditor}
            onRefresh={onRefreshEditors}
          />
        </SettingsSection>

        <SettingsSection
          icon={<Monitor />}
          title="Appearance"
          description="DECC follows the system by default."
        >
          <div className="segmented-control">
            <button
              className={settings.theme === "system" ? "segment segment--active" : "segment"}
              onClick={() => onChange({ theme: "system" })}
              type="button"
            >
              <Monitor />
              System
            </button>
            <button
              className={settings.theme === "light" ? "segment segment--active" : "segment"}
              onClick={() => onChange({ theme: "light" })}
              type="button"
            >
              <Sun />
              Light
            </button>
            <button
              className={settings.theme === "dark" ? "segment segment--active" : "segment"}
              onClick={() => onChange({ theme: "dark" })}
              type="button"
            >
              <Moon />
              Dark
            </button>
          </div>
        </SettingsSection>

        <SettingsSection
          icon={<Clock3 />}
          title="Log retention"
          description="How long completed run sessions are kept locally. Changing this also prunes older native history."
        >
          <div className="segmented-control">
            {([7, 14, 30] as const).map((days) => (
              <button
                className={settings.logRetentionDays === days ? "segment segment--active" : "segment"}
                key={days}
                onClick={() => onChange({ logRetentionDays: days })}
                type="button"
              >
                {days} days
              </button>
            ))}
          </div>
        </SettingsSection>

        <SettingsSection
          icon={<RotateCcw />}
          title="Startup"
          description="Restore context instead of reopening to an empty shell."
        >
          <ToggleRow
            label="Restore last workspace"
            detail="Open the workspace you were using when DECC closed."
            checked={settings.restoreLastWorkspace}
            onChange={(value) => onChange({ restoreLastWorkspace: value })}
          />
        </SettingsSection>

        <SettingsSection
          icon={<ShieldCheck />}
          title="Safety"
          description="Controls that help prevent accidental process changes."
        >
          <ToggleRow
            label="Confirm before Stop all"
            detail="Useful when several services are running together."
            checked={settings.confirmBeforeStopAll}
            onChange={(value) => onChange({ confirmBeforeStopAll: value })}
          />
        </SettingsSection>

      </div>

      <div className="settings-about">
        <span>DECC 0.1.0</span>
        <span>Local-first developer control center</span>
      </div>
    </div>
  );
}

function SettingsSection({
  icon,
  title,
  description,
  children,
}: {
  icon: React.ReactNode;
  title: string;
  description: string;
  children: React.ReactNode;
}) {
  return (
    <section className="settings-section">
      <div className="settings-section__identity">
        <span className="settings-section__icon">{icon}</span>
        <div>
          <h2>{title}</h2>
          <p>{description}</p>
        </div>
      </div>
      <div className="settings-section__control">{children}</div>
    </section>
  );
}

function ToggleRow({
  label,
  detail,
  checked,
  onChange,
}: {
  label: string;
  detail: string;
  checked: boolean;
  onChange: (value: boolean) => void;
}) {
  return (
    <label className="toggle-row">
      <span>
        <strong>{label}</strong>
        <small>{detail}</small>
      </span>
      <input
        checked={checked}
        onChange={(event) => onChange(event.target.checked)}
        type="checkbox"
      />
      <span className="toggle-track">
        <span />
      </span>
    </label>
  );
}
