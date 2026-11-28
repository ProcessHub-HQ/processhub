import { SearchField } from "@processhub/design-system";
import { useEffect, useRef } from "react";

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
    <SearchField
      ref={inputRef}
      className="service-search"
      size="md"
      label="Search processes"
      placeholder="Search processes…"
      value={value}
      onChange={(event) => onChange(event.target.value)}
      onClear={() => onChange("")}
      onEscape={onClose}
    />
  );
}
