import { forwardRef, type ButtonHTMLAttributes, type ReactNode } from "react";

interface IconButtonProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "children"> {
  /** Accessible name. Also shown as the tooltip. */
  label: string;
  icon: ReactNode;
  size?: "sm" | "md" | "lg";
}

export const IconButton = forwardRef<HTMLButtonElement, IconButtonProps>(function IconButton(
  { label, icon, size = "md", className, title, type = "button", ...rest },
  ref,
) {
  const classes = ["icon-btn", `icon-btn--${size}`];
  if (className) classes.push(className);
  return (
    <button
      {...rest}
      ref={ref}
      type={type}
      className={classes.join(" ")}
      aria-label={label}
      title={title ?? label}
    >
      {icon}
    </button>
  );
});
