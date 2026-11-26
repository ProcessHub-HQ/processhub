import { X } from "lucide-react";
import type { Service } from "../types";
import { Button } from "./Button";
import { Modal } from "./Modal";

interface DeleteServiceModalProps {
  service: Service;
  busy: boolean;
  onClose: () => void;
  onConfirm: () => void;
}

export function DeleteServiceModal({
  service,
  busy,
  onClose,
  onConfirm,
}: DeleteServiceModalProps) {
  return (
    <Modal
      titleId="delete-service-title"
      className="delete-dialog"
      onClose={onClose}
    >
      <div className="dialog-heading">
        <div>
          <h2 id="delete-service-title">Delete process?</h2>
          <p>“{service.name}” and its saved configuration will be removed.</p>
        </div>
        <Button
          variant="icon"
          aria-label="Close"
          icon={<X size={17} />}
          onClick={onClose}
        />
      </div>
      <div className="dialog-actions">
        <Button onClick={onClose} disabled={busy}>
          Cancel
        </Button>
        <Button variant="danger" onClick={onConfirm} disabled={busy}>
          {busy ? "Deleting…" : "Delete process"}
        </Button>
      </div>
    </Modal>
  );
}
