import { X } from "lucide-react";
import { Button } from "./Button";
import { Modal } from "./Modal";

interface AboutModalProps {
  onClose: () => void;
  websiteUrl?: string;
}

export function AboutModal({
  onClose,
  websiteUrl = "https://aakashhq.com",
}: AboutModalProps) {
  return (
    <Modal titleId="about-title" className="about-dialog" onClose={onClose}>
      <Button
        className="about-close"
        variant="icon"
        aria-label="Close About"
        onClick={onClose}
        icon={<X size={17} />}
      />
      <img className={"app-brand-image-large"} src={"/icon.svg"} alt={"Logo"} />
      <h2 id="about-title" style={{ paddingTop: "4px" }}>
        ProcessHub
      </h2>
      <p className="about-purpose">
        A local desktop workspace for managing processes and services.
      </p>
      <div className="about-details">
        <div>
          <span>Developer</span>
          <strong>Aakash Poudel</strong>
        </div>
        <div>
          <span>Version</span>
          <strong>{__APP_VERSION__}</strong>
        </div>
        <div>
          <span>Website</span>
          <a href={websiteUrl} target="_blank" rel="noreferrer">
            {websiteUrl}
          </a>
        </div>
      </div>
    </Modal>
  );
}
