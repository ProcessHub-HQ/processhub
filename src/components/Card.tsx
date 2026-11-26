import type { ButtonHTMLAttributes, ReactNode } from "react";

interface CardProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  children: ReactNode;
  selected?: boolean;
}

export function Card({
  children,
  selected = false,
  className = "",
  type = "button",
  ...props
}: CardProps) {
  return (
    <button
      {...props}
      type={type}
      className={["process-item", selected && "selected", className]
        .filter(Boolean)
        .join(" ")}
    >
      {children}
    </button>
  );
}
