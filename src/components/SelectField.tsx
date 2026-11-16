import { useId, type ReactNode, type SelectHTMLAttributes } from "react";
import { ChevronDownIcon } from "./icons";

interface SelectFieldProps extends Omit<SelectHTMLAttributes<HTMLSelectElement>, "id"> {
  label: ReactNode;
  hint?: ReactNode;
  children: ReactNode;
}

/** A native select with the look of `TextField`. */
export function SelectField({ label, hint, className, children, ...rest }: SelectFieldProps) {
  const id = useId();
  const hintId = hint ? `${id}-hint` : undefined;
  return (
    <div className={["text-field", className].filter(Boolean).join(" ")}>
      <div className="text-field__label-row">
        <label className="field-label" htmlFor={id}>
          {label}
        </label>
      </div>
      <div className="select-field__control">
        <select {...rest} id={id} className="field select-field__select" aria-describedby={hintId}>
          {children}
        </select>
        <ChevronDownIcon size="sm" className="select-field__chevron" />
      </div>
      {hint && (
        <div className="field-hint" id={hintId}>
          {hint}
        </div>
      )}
    </div>
  );
}
