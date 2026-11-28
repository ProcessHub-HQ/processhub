import { Button, Modal } from "@processhub/design-system";
import { useEffect, useState } from "react";
import { friendlyError } from "../lib/errors";
import { processHub } from "../lib/processhub";

const preferences = [
  {
    key: "startAtLogin",
    label: "Launch ProcessHub when I log in",
    defaultValue: false,
  },
  {
    key: "startMinimized",
    label: "Start ProcessHub hidden in the tray",
    defaultValue: false,
  },
  {
    key: "closeToTray",
    label: "Keep ProcessHub running when its window closes",
    defaultValue: true,
  },
  {
    key: "startAutoStartServices",
    label: "Start enabled services when ProcessHub opens",
    defaultValue: true,
  },
] as const;

type PreferenceKey = (typeof preferences)[number]["key"];
type PreferenceValues = Record<PreferenceKey, boolean>;

interface StartupSettingsModalProps {
  onClose: () => void;
}

export function StartupSettingsModal({ onClose }: StartupSettingsModalProps) {
  const [values, setValues] = useState<PreferenceValues>({
    startAtLogin: false,
    startMinimized: false,
    closeToTray: true,
    startAutoStartServices: true,
  });
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let active = true;
    void Promise.all(
      preferences.map(
        async ({ key, defaultValue }) =>
          [
            key,
            (await processHub.getSetting<boolean>(key)) ?? defaultValue,
          ] as const,
      ),
    )
      .then((entries) => {
        if (active) setValues(Object.fromEntries(entries) as PreferenceValues);
      })
      .catch((cause: unknown) => {
        if (active) {
          setError(
            friendlyError(
              cause,
              "ProcessHub couldn't load your settings. Try again.",
            ),
          );
        }
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, []);

  async function update(key: PreferenceKey, value: boolean) {
    setError(null);
    try {
      await processHub.setSetting(key, value);
      setValues((current) => ({ ...current, [key]: value }));
    } catch (cause) {
      setError(
        friendlyError(cause, "That setting couldn't be saved. Try again."),
      );
    }
  }

  return (
    <Modal
      titleId="startup-settings-title"
      className="startup-settings-dialog"
      onClose={onClose}
    >
      <div className="dialog-heading">
        <div>
          <h2 id="startup-settings-title">Startup &amp; tray</h2>
          <p>Choose what ProcessHub does when it opens or closes.</p>
        </div>
        <Button size="sm" onClick={onClose}>
          Done
        </Button>
      </div>
      {preferences.map(({ key, label }) => (
        <label className="startup-setting" key={key}>
          <span>{label}</span>
          <input
            type="checkbox"
            checked={values[key]}
            disabled={loading}
            onChange={(event) => void update(key, event.target.checked)}
          />
        </label>
      ))}
      {error && (
        <p className="form-error" role="alert">
          {error}
        </p>
      )}
    </Modal>
  );
}
