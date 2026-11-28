import {
  Button,
  Checkbox,
  Input,
  Modal,
  Select,
  Textarea,
} from "@processhub/design-system";
import { open } from "@tauri-apps/plugin-dialog";
import { ChevronDown, ChevronRight, X } from "lucide-react";
import { type FormEvent, useState } from "react";
import { friendlyError } from "../lib/errors";
import { newDraftId } from "../lib/ids";
import type { ServiceDraft } from "../stores/servicesStore";
import type { Service } from "../types";
import { ImageInput } from "./ImageInput";
import { SecretEditor } from "./SecretEditor";

interface ServiceFormModalProps {
  service?: Service;
  services: Service[];
  busy: boolean;
  onClose: () => void;
  onSave: (draft: ServiceDraft) => Promise<boolean>;
}

function hasCustomAdvanced(service?: Service) {
  if (!service) return false;
  return (
    service.restartPolicy !== "never" ||
    service.stopGracePeriodMs !== 3000 ||
    service.logMaxSizeMb !== 10 ||
    service.healthCheckType !== "none" ||
    service.enabled === false ||
    service.autoStart ||
    (service.description ?? "").trim() !== "" ||
    service.dependsOn.length > 0 ||
    service.environment.length > 0
  );
}

function describeAdvanced(draft: ServiceDraft) {
  const items: string[] = [];
  if (draft.restartPolicy === "on_failure") items.push("Restart on failure");
  if (draft.restartPolicy === "always") items.push("Restart always");
  if (draft.healthCheckType !== "none") items.push("Health check");
  if (draft.environmentText.trim() || draft.secrets.length > 0) {
    items.push("Environment");
  }
  if (draft.dependsOn.length > 0) items.push("Startup order");
  if (draft.description.trim() !== "") items.push("Description");
  if (draft.autoStart) items.push("Auto-start");
  if (!draft.enabled) items.push("Disabled");
  return items.length > 0 ? items.join(" · ") : "Defaults";
}

export function ServiceFormModal({
  service,
  services,
  busy,
  onClose,
  onSave,
}: ServiceFormModalProps) {
  const [draft, setDraft] = useState<ServiceDraft>(() => ({
    id: service?.id,
    name: service?.name ?? "",
    description: service?.description ?? "",
    iconPath: service?.iconPath ?? null,
    dependsOn: service?.dependsOn ?? [],
    executable: service?.executable ?? "",
    argumentsText: service?.arguments.join("\n") ?? "",
    workingDirectory: service?.workingDirectory ?? "",
    environmentText:
      service?.environment
        .filter((variable) => !variable.isSecret)
        .map(({ key, value }) => `${key}=${value}`)
        .join("\n") ?? "",
    secrets:
      service?.environment
        .filter((variable) => variable.isSecret)
        .map(({ key }) => ({
          id: newDraftId("secret"),
          key,
          value: "",
          existing: true,
        })) ?? [],
    restartPolicy: service?.restartPolicy ?? "never",
    restartDelayMs: service?.restartDelayMs ?? 1000,
    maxRestartAttempts: service?.maxRestartAttempts ?? 5,
    stopGracePeriodMs: service?.stopGracePeriodMs ?? 3000,
    logMaxSizeMb: service?.logMaxSizeMb ?? 10,
    healthCheckType: service?.healthCheckType ?? "none",
    healthTarget: service?.healthTarget ?? "",
    healthIntervalMs: service?.healthIntervalMs ?? 5000,
    healthTimeoutMs: service?.healthTimeoutMs ?? 2000,
    healthExpectedStatus: service?.healthExpectedStatus ?? null,
    healthStartPeriodMs: service?.healthStartPeriodMs ?? 5000,
    enabled: service?.enabled ?? true,
    autoStart: service?.autoStart ?? false,
  }));
  const [showAdvanced, setShowAdvanced] = useState(() =>
    hasCustomAdvanced(service),
  );
  const [formError, setFormError] = useState<string | null>(null);
  const dependencyOptions = services.filter(
    (item) => item.id !== (draft.id ?? service?.id),
  );
  const advancedSummary = describeAdvanced(draft);

  async function chooseWorkingDirectory() {
    try {
      const selected = await open({ directory: true, multiple: false });
      if (typeof selected === "string")
        setDraft((current) => ({ ...current, workingDirectory: selected }));
    } catch (cause) {
      setFormError(
        friendlyError(
          cause,
          "The folder picker couldn't be opened. Try again.",
        ),
      );
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
            size="xs"
            variant="icon"
            aria-label="Close"
            icon={<X size={17} />}
            onClick={onClose}
          />
        </div>
        <div className="service-form-body">
          <div className="service-form-grid">
            <ImageInput
              label="Icon"
              value={draft.iconPath}
              onChange={(iconPath) => setDraft({ ...draft, iconPath })}
              onError={setFormError}
            />
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
              <Button size="xs" onClick={() => void chooseWorkingDirectory()}>
                Browse
              </Button>
            </div>
          </div>
          <Textarea
            label="Arguments"
            hint="One argument per line"
            rows={2}
            value={draft.argumentsText}
            onChange={(event) =>
              setDraft({ ...draft, argumentsText: event.target.value })
            }
            placeholder={"server.js\n--port\n3000"}
          />
          <button
            type="button"
            className="advanced-toggle"
            aria-expanded={showAdvanced}
            aria-controls="service-advanced"
            onClick={() => setShowAdvanced((current) => !current)}
          >
            {showAdvanced ? (
              <ChevronDown size={16} aria-hidden="true" />
            ) : (
              <ChevronRight size={16} aria-hidden="true" />
            )}
            <span>Advanced settings</span>
            <span className="advanced-toggle__summary">{advancedSummary}</span>
          </button>
          {showAdvanced && (
            <div className="advanced-panel" id="service-advanced">
              <section className="advanced-group">
                <h3 className="form-section-title">Details</h3>
                <Input
                  label="Description"
                  hint="Optional"
                  maxLength={500}
                  value={draft.description}
                  onChange={(event) =>
                    setDraft({ ...draft, description: event.target.value })
                  }
                  placeholder="What this process does"
                />
              </section>
              <section className="advanced-group">
                <h3 className="form-section-title">Environment</h3>
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
                <SecretEditor
                  secrets={draft.secrets}
                  onChange={(secrets) => setDraft({ ...draft, secrets })}
                />
              </section>
              <section className="advanced-group">
                <h3 className="form-section-title">Behavior</h3>
                <div className="form-options">
                  <Select
                    label="Restart policy"
                    value={draft.restartPolicy}
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
                  </Select>
                  {draft.restartPolicy !== "never" && (
                    <Input
                      label="Restart delay (ms)"
                      hint="Backs off up to 30 s"
                      type="number"
                      min={0}
                      value={draft.restartDelayMs}
                      onChange={(event) =>
                        setDraft({
                          ...draft,
                          restartDelayMs: Number(event.target.value),
                        })
                      }
                    />
                  )}
                  {draft.restartPolicy !== "never" && (
                    <Input
                      label="Max restart attempts"
                      hint="Blank means unlimited"
                      type="number"
                      min={1}
                      value={draft.maxRestartAttempts ?? ""}
                      onChange={(event) =>
                        setDraft({
                          ...draft,
                          maxRestartAttempts:
                            event.target.value === ""
                              ? null
                              : Math.max(
                                  1,
                                  Math.floor(Number(event.target.value)),
                                ),
                        })
                      }
                    />
                  )}
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
                  <Checkbox
                    label="Enabled"
                    checked={draft.enabled}
                    onChange={(event) =>
                      setDraft({ ...draft, enabled: event.target.checked })
                    }
                  />
                  <Checkbox
                    label="Start automatically"
                    checked={draft.autoStart}
                    onChange={(event) =>
                      setDraft({ ...draft, autoStart: event.target.checked })
                    }
                  />
                </div>
                <p className="advanced-hint">
                  Auto-start runs this process when ProcessHub opens.
                </p>
              </section>
              {dependencyOptions.length > 0 && (
                <section className="advanced-group">
                  <h3 className="form-section-title">Startup order</h3>
                  <div className="dependency-list">
                    <span className="dependency-list-label">Start after</span>
                    <div className="dependency-options">
                      {dependencyOptions.map((item) => (
                        <Checkbox
                          key={item.id}
                          className="dependency-option"
                          label={item.name}
                          checked={draft.dependsOn.includes(item.id)}
                          onChange={(event) =>
                            setDraft({
                              ...draft,
                              dependsOn: event.target.checked
                                ? [...draft.dependsOn, item.id]
                                : draft.dependsOn.filter(
                                    (id) => id !== item.id,
                                  ),
                            })
                          }
                        />
                      ))}
                    </div>
                  </div>
                </section>
              )}
              <section className="advanced-group">
                <h3 className="form-section-title">Health check</h3>
                <div className="form-options">
                  <Select
                    label="Check"
                    value={draft.healthCheckType}
                    onChange={(event) =>
                      setDraft({
                        ...draft,
                        healthCheckType: event.target
                          .value as ServiceDraft["healthCheckType"],
                      })
                    }
                  >
                    <option value="none">Not configured</option>
                    <option value="tcp">TCP port</option>
                    <option value="http">HTTP URL</option>
                  </Select>
                  {draft.healthCheckType !== "none" && (
                    <Input
                      label={
                        draft.healthCheckType === "http" ? "URL" : "Target"
                      }
                      hint={
                        draft.healthCheckType === "http"
                          ? "http://host:port/path"
                          : "Port or host:port"
                      }
                      value={draft.healthTarget}
                      onChange={(event) =>
                        setDraft({ ...draft, healthTarget: event.target.value })
                      }
                      placeholder={
                        draft.healthCheckType === "http"
                          ? "http://localhost:3000/health"
                          : "3000"
                      }
                    />
                  )}
                  {draft.healthCheckType !== "none" && (
                    <Input
                      label="Check interval (ms)"
                      type="number"
                      min={500}
                      max={300000}
                      value={draft.healthIntervalMs}
                      onChange={(event) =>
                        setDraft({
                          ...draft,
                          healthIntervalMs: Number(event.target.value),
                        })
                      }
                    />
                  )}
                  {draft.healthCheckType !== "none" && (
                    <Input
                      label="Check timeout (ms)"
                      type="number"
                      min={100}
                      max={60000}
                      value={draft.healthTimeoutMs}
                      onChange={(event) =>
                        setDraft({
                          ...draft,
                          healthTimeoutMs: Number(event.target.value),
                        })
                      }
                    />
                  )}
                  {draft.healthCheckType !== "none" && (
                    <Input
                      label="Start period (ms)"
                      type="number"
                      min={0}
                      max={600000}
                      value={draft.healthStartPeriodMs}
                      onChange={(event) =>
                        setDraft({
                          ...draft,
                          healthStartPeriodMs: Number(event.target.value),
                        })
                      }
                    />
                  )}
                  {draft.healthCheckType === "http" && (
                    <Input
                      label="Expected status"
                      hint="Blank accepts 2xx–3xx"
                      type="number"
                      min={100}
                      max={599}
                      value={draft.healthExpectedStatus ?? ""}
                      onChange={(event) =>
                        setDraft({
                          ...draft,
                          healthExpectedStatus:
                            event.target.value === ""
                              ? null
                              : Number(event.target.value),
                        })
                      }
                    />
                  )}
                </div>
              </section>
            </div>
          )}
          {formError && (
            <p className="form-error" role="alert">
              {formError}
            </p>
          )}
        </div>
        <div className="dialog-actions modal-fixed-footer">
          <Button size="sm" onClick={onClose}>
            Cancel
          </Button>
          <Button size="sm" variant="primary" type="submit" disabled={busy}>
            {busy ? "Saving…" : service ? "Save changes" : "Save process"}
          </Button>
        </div>
      </form>
    </Modal>
  );
}
