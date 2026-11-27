import { convertFileSrc } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { ImagePlus, Upload, X } from "lucide-react";
import { useState } from "react";
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
      onError?.(String(cause));
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
          <button
            type="button"
            className="image-input-button remove"
            aria-label="Remove image"
            title="Remove image"
            disabled={busy}
            onClick={() => onChange(null)}
          >
            <X size={12} />
          </button>
        )}
      </div>
    </div>
  );
}
