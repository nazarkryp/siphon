import { forwardRef, useId, type InputHTMLAttributes, type ReactNode } from "react";

interface TextFieldProps extends Omit<InputHTMLAttributes<HTMLInputElement>, "id"> {
  label: ReactNode;
  hint?: ReactNode;
  /** Replaces the hint and marks the field as invalid. */
  error?: string | null;
  /** Controls after the input, for example a "Browse" button. */
  trailing?: ReactNode;
  /** Content at the end of the label row, for example a status pill. */
  labelAside?: ReactNode;
}

export const TextField = forwardRef<HTMLInputElement, TextFieldProps>(function TextField(
  { label, hint, error, trailing, labelAside, className, type = "text", ...rest },
  ref,
) {
  const id = useId();
  const hintId = hint || error ? `${id}-hint` : undefined;
  return (
    <div className={["text-field", className].filter(Boolean).join(" ")}>
      <div className="text-field__label-row">
        <label className="field-label" htmlFor={id}>
          {label}
        </label>
        {labelAside}
      </div>
      <div className="text-field__control">
        <input
          {...rest}
          ref={ref}
          id={id}
          type={type}
          className="field"
          aria-describedby={hintId}
          aria-invalid={error ? true : undefined}
          spellCheck={false}
        />
        {trailing}
      </div>
      {error ? (
        <div className="field-error selectable" id={hintId} role="alert">
          {error}
        </div>
      ) : (
        hint && (
          <div className="field-hint" id={hintId}>
            {hint}
          </div>
        )
      )}
    </div>
  );
});
