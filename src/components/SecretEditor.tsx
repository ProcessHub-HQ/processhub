import { Plus, X } from "lucide-react";
import { newDraftId } from "../lib/ids";
import type { SecretDraft } from "../types";
import { Button } from "./Button";

interface SecretEditorProps {
  secrets: SecretDraft[];
  onChange: (secrets: SecretDraft[]) => void;
}

export function SecretEditor({ secrets, onChange }: SecretEditorProps) {
  return (
    <div className="secret-editor">
      <span className="dependency-list-label">Secrets</span>
      {secrets.map((secret, index) => (
        <div className="secret-row" key={secret.id}>
          <input
            aria-label="Secret name"
            maxLength={128}
            placeholder="TOKEN"
            value={secret.key}
            disabled={secret.existing}
            onChange={(event) =>
              onChange(
                secrets.map((item, itemIndex) =>
                  itemIndex === index
                    ? { ...item, key: event.target.value }
                    : item,
                ),
              )
            }
          />
          <input
            aria-label={`Value for ${secret.key || "secret"}`}
            type="password"
            autoComplete="new-password"
            placeholder={secret.existing ? "Stored securely" : "Value"}
            value={secret.value}
            onChange={(event) =>
              onChange(
                secrets.map((item, itemIndex) =>
                  itemIndex === index
                    ? { ...item, value: event.target.value }
                    : item,
                ),
              )
            }
          />
          <button
            type="button"
            className="image-input-button remove"
            aria-label={`Remove ${secret.key || "secret"}`}
            title="Remove secret"
            onClick={() =>
              onChange(secrets.filter((_, itemIndex) => itemIndex !== index))
            }
          >
            <X size={12} />
          </button>
        </div>
      ))}
      <Button
        icon={<Plus size={12} />}
        onClick={() =>
          onChange([
            ...secrets,
            {
              id: newDraftId("secret"),
              key: "",
              value: "",
              existing: false,
            },
          ])
        }
      >
        Add secret
      </Button>
    </div>
  );
}
