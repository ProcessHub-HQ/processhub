import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

function appWindow() {
  return isTauri() ? getCurrentWindow() : null;
}

function hasNativeMacControls() {
  return isTauri() && navigator.userAgent.includes("Macintosh");
}

export function WindowControls() {
  if (hasNativeMacControls()) return null;

  return (
    <div className="window-controls">
      <button
        type="button"
        className="window-control window-control--close"
        aria-label="Close window"
        title="Close"
        onClick={() => void appWindow()?.close()}
      >
        <svg viewBox="0 0 8 8" aria-hidden="true">
          <path
            d="M1.2 1.2l5.6 5.6M6.8 1.2L1.2 6.8"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.4"
            strokeLinecap="round"
          />
        </svg>
      </button>
      <button
        type="button"
        className="window-control window-control--minimize"
        aria-label="Minimize window"
        title="Minimize"
        onClick={() => void appWindow()?.minimize()}
      >
        <svg viewBox="0 0 8 8" aria-hidden="true">
          <path
            d="M1.2 4h5.6"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.4"
            strokeLinecap="round"
          />
        </svg>
      </button>
      <button
        type="button"
        className="window-control window-control--zoom"
        aria-label="Zoom window"
        title="Window size is fixed"
        disabled
      >
        <svg viewBox="0 0 8 8" aria-hidden="true">
          <path
            d="M4 1.2v5.6M1.2 4h5.6"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.4"
            strokeLinecap="round"
          />
        </svg>
      </button>
    </div>
  );
}
