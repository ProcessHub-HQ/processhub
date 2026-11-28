import { IconButton } from "@processhub/design-system";
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
      <IconButton
        size="xs"
        variant="danger"
        label="Dismiss error"
        onClick={onDismiss}
      >
        <X size={14} />
      </IconButton>
    </div>
  );
}
