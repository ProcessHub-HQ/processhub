import { Button, Modal } from "@processhub/design-system";
import {
  AlertCircle,
  CheckCircle2,
  Download,
  RefreshCw,
  X,
} from "lucide-react";
import type { ReactNode } from "react";
import { useUiStore } from "../stores/uiStore";

const LINK_PATTERN = /\[([^\]]+)\]\((https?:\/\/[^)]+)\)/g;

function renderInline(text: string): ReactNode[] {
  const parts: ReactNode[] = [];
  const source = text.replace(/\*\*/g, "");
  let cursor = 0;
  let key = 0;
  for (const match of source.matchAll(LINK_PATTERN)) {
    const index = match.index ?? 0;
    if (index > cursor) parts.push(source.slice(cursor, index));
    parts.push(
      <a
        key={key}
        href={match[2]}
        target="_blank"
        rel="noreferrer"
        onClick={(event) => event.stopPropagation()}
      >
        {match[1].replace(/`/g, "")}
      </a>,
    );
    key += 1;
    cursor = index + match[0].length;
  }
  if (cursor < source.length) parts.push(source.slice(cursor));
  return parts;
}

function UpdateNotes({ notes }: { notes: string | null }) {
  if (!notes?.trim()) return null;
  return (
    <div className="update-notes">
      {notes.split("\n").map((line, index) => {
        const key = `${index}-${line.slice(0, 16)}`;
        if (line.startsWith("### ")) return <h4 key={key}>{line.slice(4)}</h4>;
        if (line.startsWith("## ")) return <h3 key={key}>{line.slice(3)}</h3>;
        if (line.trim() === "---") return <hr key={key} />;
        if (line.startsWith("- ")) {
          return (
            <p className="update-note-item" key={key}>
              <span aria-hidden="true">•</span>
              <span>{renderInline(line.slice(2))}</span>
            </p>
          );
        }
        if (!line.trim()) return null;
        return <p key={key}>{renderInline(line)}</p>;
      })}
    </div>
  );
}

function heroTitle(phase: string, version?: string | null) {
  if (phase === "checking") return "Checking for updates…";
  if (phase === "current") return "You’re up to date";
  if (phase === "available") {
    return version ? `ProcessHub ${version} is available` : "Update available";
  }
  if (phase === "downloading") return "Downloading update…";
  if (phase === "installing") return "Installing update…";
  if (phase === "restarting") return "Restarting ProcessHub…";
  if (phase === "error") return "Update failed";
  return "ProcessHub update";
}

function HeroIcon({ phase }: { phase: string }) {
  if (phase === "current") return <CheckCircle2 size={28} />;
  if (phase === "error") return <AlertCircle size={28} />;
  if (phase === "available") return <Download size={28} />;
  return <RefreshCw size={28} />;
}

export function UpdateDialog() {
  const open = useUiStore((state) => state.updateOpen);
  const phase = useUiStore((state) => state.updatePhase);
  const update = useUiStore((state) => state.availableUpdate);
  const progress = useUiStore((state) => state.updateProgress);
  const updateError = useUiStore((state) => state.updateError);
  const close = useUiStore((state) => state.closeUpdateDialog);
  const install = useUiStore((state) => state.installUpdate);
  const check = useUiStore((state) => state.checkForUpdates);

  if (!open) return null;

  const busy = ["downloading", "installing", "restarting"].includes(phase);
  const locked = busy || phase === "checking";
  const percent =
    progress?.total && progress.total > 0
      ? Math.min(100, Math.round((progress.downloaded / progress.total) * 100))
      : null;
  const currentVersion = update?.currentVersion ?? __APP_VERSION__;

  return (
    <Modal
      titleId="update-title"
      className={`update-dialog update-dialog-${phase}`}
      onClose={close}
    >
      <h2 id="update-title" className="visually-hidden">
        ProcessHub update
      </h2>
      <Button
        size="xs"
        className="update-close"
        variant="icon"
        aria-label="Close update dialog"
        disabled={locked}
        icon={<X size={17} />}
        onClick={close}
      />

      <div className="update-body">
        <div
          className={`update-hero update-hero-${phase}`}
          role={phase === "error" ? "alert" : "status"}
        >
          <span className="update-hero-icon" aria-hidden="true">
            <HeroIcon phase={phase} />
          </span>
          <strong>{heroTitle(phase, update?.version)}</strong>
          {phase === "current" && (
            <span>ProcessHub v{currentVersion} is the latest version.</span>
          )}
          {phase === "available" && update && (
            <span className="update-version-sub">
              v{update.currentVersion} <span aria-hidden="true">→</span>{" "}
              <strong>v{update.version}</strong>
            </span>
          )}
          {phase === "error" && updateError && <span>{updateError}</span>}
        </div>

        {phase === "checking" && (
          <div
            className="update-checking"
            role="status"
            aria-label="Checking for updates"
          >
            <div className="update-progress indeterminate">
              <span />
            </div>
          </div>
        )}

        {phase === "available" && update && (
          <>
            <p className="update-caution">
              Running services will be stopped before installation.
            </p>
            <UpdateNotes notes={update.notes} />
          </>
        )}

        {busy && (
          <div className="update-busy" role="status">
            <div
              className={`update-progress ${percent === null ? "indeterminate" : ""}`}
            >
              <span
                style={percent === null ? undefined : { width: `${percent}%` }}
              />
            </div>
            {percent !== null && (
              <span className="update-progress-value">{percent}%</span>
            )}
          </div>
        )}
      </div>

      <div className="dialog-actions update-footer">
        {phase === "available" && (
          <Button
            size="xs"
            variant="primary"
            icon={<Download size={13} />}
            onClick={() => void install()}
          >
            Download &amp; install
          </Button>
        )}
        {phase === "error" && (
          <Button
            size="xs"
            variant="primary"
            icon={<RefreshCw size={13} />}
            onClick={() => void check()}
          >
            Try again
          </Button>
        )}
      </div>
    </Modal>
  );
}
