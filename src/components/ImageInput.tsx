import { IconButton } from "@processhub/design-system";
import { convertFileSrc } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { ImagePlus, Upload, X } from "lucide-react";
import { useState } from "react";
import { friendlyError } from "../lib/errors";
import { processHub } from "../lib/processhub";

interface ImageInputProps {
  label: string;
  value: string | null;
  onChange: (value: string | null) => void;
  onError?: (message: string) => void;
}

export function ImageInput({
  label,
  value,
  onChange,
  onError,
}: ImageInputProps) {
  const [busy, setBusy] = useState(false);

  async function choose() {
    try {
      const selected = await open({
        multiple: false,
        filters: [
          { name: "Image", extensions: ["png", "jpg", "jpeg", "webp"] },
        ],
      });
      if (typeof selected !== "string") return;
      setBusy(true);
      const iconPath = await processHub.importServiceIcon(selected);
      onChange(iconPath);
    } catch (cause) {
      onError?.(
        friendlyError(
          cause,
          "The icon couldn't be saved. Choose a PNG, JPEG, or WebP under 8 MB.",
        ),
      );
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="ph-field image-field">
      <span>{label}</span>
      <div className="image-input-control">
        <span className="image-input-preview" aria-hidden="true">
          {value ? (
            <img src={convertFileSrc(value)} alt="" />
          ) : (
            <ImagePlus size={15} strokeWidth={1.8} />
          )}
        </span>
        <span className="image-input-status">
          {value ? "Image selected" : "No image selected"}
        </span>
        <button
          type="button"
          className="image-input-button"
          disabled={busy}
          onClick={() => void choose()}
        >
          <Upload size={12} />
          {value ? "Replace" : "Choose"}
        </button>
        {value && (
          <IconButton
            size="xs"
            variant="danger"
            label="Remove image"
            title="Remove image"
            disabled={busy}
            onClick={() => onChange(null)}
          >
            <X size={12} />
          </IconButton>
        )}
      </div>
    </div>
  );
}
