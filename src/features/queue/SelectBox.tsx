import { useEffect, useRef } from "react";

interface SelectBoxProps {
  label: string;
  checked: boolean;
  /** Some rows are selected, not all. */
  mixed?: boolean;
  onChange: () => void;
}

/** A checkbox whose clickable area fills the whole select cell. */
export function SelectBox({ label, checked, mixed = false, onChange }: SelectBoxProps) {
  const ref = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (ref.current) ref.current.indeterminate = mixed;
  }, [mixed]);

  return (
    <label className="queue-select" title={label}>
      <input ref={ref} type="checkbox" aria-label={label} checked={checked} onChange={onChange} />
    </label>
  );
}
