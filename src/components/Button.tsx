import type { ButtonHTMLAttributes, ReactNode } from "react";

type ButtonVariant = "new" | "icon" | "primary" | "secondary" | "danger";

const variantClass: Record<ButtonVariant, string> = {
  new: "new-button",
  icon: "icon-button",
  primary: "save-button",
  secondary: "cancel-button",
  danger: "danger-button",
};

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  icon?: ReactNode;
}

export function Button({
  variant = "secondary",
  icon,
  className = "",
  children,
  type = "button",
  ...props
}: ButtonProps) {
  return (
    <button
      {...props}
      type={type}
      className={[variantClass[variant], className].filter(Boolean).join(" ")}
    >
      {icon}
      {children}
    </button>
  );
}
