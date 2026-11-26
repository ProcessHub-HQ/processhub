import { open } from "@tauri-apps/plugin-dialog";
import { X } from "lucide-react";
import { type FormEvent, useState } from "react";
import type { ServiceDraft } from "../hooks/useServiceManager";
import { processHub } from "../lib/processhub";
import type { Service } from "../types";
import { Button } from "./Button";
import { Input, Textarea } from "./Input";
import { Modal } from "./Modal";

interface ServiceFormModalProps {
  service?: Service;
  busy: boolean;
  onClose: () => void;
  onSave: (draft: ServiceDraft) => Promise<boolean>;
}

export function ServiceFormModal({
  service,
  busy,
  onClose,
  onSave,
}: ServiceFormModalProps) {
  const [draft, setDraft] = useState<ServiceDraft>(() => ({
    id: service?.id,
    name: service?.name ?? "",
    description: service?.description ?? "",
    iconPath: service?.iconPath ?? null,
    executable: service?.executable ?? "",
    argumentsText: service?.arguments.join("\n") ?? "",
    workingDirectory: service?.workingDirectory ?? "",
    environmentText:
      service?.environment
        .map(({ key, value }) => `${key}=${value}`)
        .join("\n") ?? "",
    restartPolicy: service?.restartPolicy ?? "never",
    restartDelayMs: service?.restartDelayMs ?? 1000,
    stopGracePeriodMs: service?.stopGracePeriodMs ?? 3000,
    logMaxSizeMb: service?.logMaxSizeMb ?? 10,
    enabled: service?.enabled ?? true,
    autoStart: service?.autoStart ?? false,
  }));
  const [formError, setFormError] = useState<string | null>(null);

  async function chooseIcon() {
    try {
      const selected = await open({
        multiple: false,
        filters: [
          { name: "Image", extensions: ["png", "jpg", "jpeg", "webp"] },
        ],
      });
      if (typeof selected === "string") {
        const iconPath = await processHub.importServiceIcon(selected);
        setDraft((current) => ({ ...current, iconPath }));
        setFormError(null);
      }
    } catch (cause) {
      setFormError(String(cause));
    }
  }

  async function chooseWorkingDirectory() {
    try {
      const selected = await open({ directory: true, multiple: false });
      if (typeof selected === "string")
        setDraft((current) => ({ ...current, workingDirectory: selected }));
    } catch (cause) {
      setFormError(String(cause));
    }
  }

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (await onSave(draft)) onClose();
  }

  return (
    <Modal
      titleId="service-dialog-title"
      className="service-dialog"
      onClose={onClose}
    >
      <form
        className="service-form-shell"
        onSubmit={(event) => void submit(event)}
      >
        <div className="dialog-heading modal-fixed-header">
          <div>
            <h2 id="service-dialog-title">
              {service ? "Edit process" : "Add process"}
            </h2>
            <p>Configure the command ProcessHub will run.</p>
          </div>
          <Button
            variant="icon"
            aria-label="Close"
            icon={<X size={17} />}
            onClick={onClose}
          />
        </div>
        <div className="service-form-body">
          <div className="form-icon-row">
            <span>
              {draft.iconPath
                ? "Custom icon selected"
                : "Use a custom process icon"}
            </span>
            <Button onClick={() => void chooseIcon()}>Choose icon</Button>
          </div>
          <div className="service-form-grid">
            <Input
              label="Name"
              autoFocus
              required
              maxLength={120}
              value={draft.name}
              onChange={(event) =>
                setDraft({ ...draft, name: event.target.value })
              }
              placeholder="Local API"
            />
            <Input
              label="Description"
              maxLength={500}
              value={draft.description}
              onChange={(event) =>
                setDraft({ ...draft, description: event.target.value })
              }
              placeholder="What this process does"
            />
            <Input
              label="Executable"
              required
              value={draft.executable}
              onChange={(event) =>
                setDraft({ ...draft, executable: event.target.value })
              }
              placeholder="node or /path/to/program"
            />
            <div className="path-picker-row">
              <Input
                label="Working directory"
                hint="Optional"
                value={draft.workingDirectory}
                onChange={(event) =>
                  setDraft({ ...draft, workingDirectory: event.target.value })
                }
                placeholder="/path/to/project"
              />
              <Button onClick={() => void chooseWorkingDirectory()}>
                Browse
              </Button>
            </div>
          </div>
          <Textarea
            label="Arguments"
            hint="One argument per line"
            rows={4}
            value={draft.argumentsText}
            onChange={(event) =>
              setDraft({ ...draft, argumentsText: event.target.value })
            }
            placeholder={"server.js\n--port\n3000"}
          />
          <div className="advanced-fields">
            <h3 className="form-section-title">Environment &amp; behavior</h3>
            <Textarea
              label="Environment variables"
              hint="One KEY=VALUE per line"
              rows={3}
              value={draft.environmentText}
              onChange={(event) =>
                setDraft({ ...draft, environmentText: event.target.value })
              }
              placeholder={"NODE_ENV=development\nPORT=3000"}
            />
            <div className="form-options">
              <label>
                Execution mode
                <select value="direct" disabled>
                  <option value="direct">Direct executable</option>
                  <option value="shell" disabled>
                    Shell (not available)
                  </option>
                </select>
              </label>
              <label>
                Restart policy
                <select
                  value={draft.restartPolicy}
                  disabled
                  onChange={(event) =>
                    setDraft({
                      ...draft,
                      restartPolicy: event.target
                        .value as ServiceDraft["restartPolicy"],
                    })
                  }
                >
                  <option value="never">Never</option>
                  <option value="on_failure">On failure</option>
                  <option value="always">Always</option>
                </select>
              </label>
              <Input
                label="Restart delay (ms)"
                type="number"
                min={0}
                disabled
                value={draft.restartDelayMs}
                onChange={(event) =>
                  setDraft({
                    ...draft,
                    restartDelayMs: Number(event.target.value),
                  })
                }
              />
              <Input
                label="Stop grace period (ms)"
                type="number"
                min={0}
                value={draft.stopGracePeriodMs}
                onChange={(event) =>
                  setDraft({
                    ...draft,
                    stopGracePeriodMs: Number(event.target.value),
                  })
                }
              />
              <Input
                label="Max log size (MB)"
                hint="Rotates per file (1–1024)"
                type="number"
                min={1}
                max={1024}
                value={draft.logMaxSizeMb}
                onChange={(event) =>
                  setDraft({
                    ...draft,
                    logMaxSizeMb: Number(event.target.value),
                  })
                }
              />
            </div>
            <div className="form-toggles">
              <label>
                <input
                  type="checkbox"
                  checked={draft.enabled}
                  onChange={(event) =>
                    setDraft({ ...draft, enabled: event.target.checked })
                  }
                />{" "}
                Enabled
              </label>
              <label>
                <input
                  type="checkbox"
                  checked={draft.autoStart}
                  onChange={(event) =>
                    setDraft({ ...draft, autoStart: event.target.checked })
                  }
                />{" "}
                Start automatically
              </label>
            </div>
            <p className="unavailable-feature">
              Enabled services with auto-start on will launch when ProcessHub
              opens.
            </p>
            <div className="command-preview">
              <span>Command preview</span>
              <code>
                {[
                  draft.executable,
                  ...draft.argumentsText.split("\n").filter(Boolean),
                ]
                  .filter(Boolean)
                  .map((part) => JSON.stringify(part))
                  .join(" ") || "Enter an executable"}
              </code>
            </div>
          </div>
          {formError && (
            <p className="form-error" role="alert">
              {formError}
            </p>
          )}
        </div>
        <div className="dialog-actions modal-fixed-footer">
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" type="submit" disabled={busy}>
            {busy ? "Saving…" : service ? "Save changes" : "Save process"}
          </Button>
        </div>
      </form>
    </Modal>
  );
}
