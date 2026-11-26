import { X } from "lucide-react";

interface ServiceErrorProps {
  message: string;
  onRetry?: () => void;
  onDismiss: () => void;
}

export function ServiceError({
  message,
  onRetry,
  onDismiss,
}: ServiceErrorProps) {
  return (
    <div className="error-banner" role="alert">
      <span>{message}</span>
      {onRetry && (
        <button className="error-retry" type="button" onClick={onRetry}>
          Retry
        </button>
      )}
      <button
        className="error-close"
        type="button"
        aria-label="Dismiss error"
        onClick={onDismiss}
      >
        <X size={14} />
      </button>
    </div>
  );
}
