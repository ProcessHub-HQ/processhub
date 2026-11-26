import { Search, X } from "lucide-react";
import { useEffect, useRef } from "react";
import { Button } from "./Button";

interface ServiceSearchProps {
  value: string;
  onChange: (value: string) => void;
  onClose: () => void;
}

export function ServiceSearch({
  value,
  onChange,
  onClose,
}: ServiceSearchProps) {
  const inputRef = useRef<HTMLInputElement>(null);
  useEffect(() => inputRef.current?.focus(), []);

  return (
    <search className="service-search">
      <Search size={15} aria-hidden="true" />
      <input
        ref={inputRef}
        type="search"
        aria-label="Search processes"
        placeholder="Search processes…"
        value={value}
        onChange={(event) => onChange(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Escape") onClose();
        }}
      />
      <Button
        variant="icon"
        aria-label="Close search"
        icon={<X size={15} />}
        onClick={onClose}
      />
    </search>
  );
}
