import type { ButtonHTMLAttributes, ReactNode } from "react";
import { SpinnerIcon } from "./icons";

type Variant = "primary" | "secondary" | "ghost" | "link";

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant;
  icon?: ReactNode;
  /** Shows a spinner and blocks clicks while an action runs. */
  loading?: boolean;
}

export function Button({
  variant = "secondary",
  icon,
  loading = false,
  disabled,
  className,
  children,
  type = "button",
  ...rest
}: ButtonProps) {
  const classes = ["btn", `btn--${variant}`];
  if (className) classes.push(className);
  return (
    <button
      {...rest}
      type={type}
      className={classes.join(" ")}
      disabled={disabled || loading}
      aria-busy={loading || undefined}
    >
      {loading ? <SpinnerIcon spin /> : icon}
      {children}
    </button>
  );
}
