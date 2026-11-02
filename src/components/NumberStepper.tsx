import { MinusIcon, PlusIcon } from "./icons";

interface NumberStepperProps {
  value: number;
  min: number;
  max: number;
  onChange: (value: number) => void;
  labelledBy: string;
  decreaseLabel: string;
  increaseLabel: string;
}

export function NumberStepper({
  value,
  min,
  max,
  onChange,
  labelledBy,
  decreaseLabel,
  increaseLabel,
}: NumberStepperProps) {
  return (
    <div className="stepper" role="group" aria-labelledby={labelledBy}>
      <button
        type="button"
        className="stepper__button"
        aria-label={decreaseLabel}
        title={decreaseLabel}
        disabled={value <= min}
        onClick={() => onChange(Math.max(min, value - 1))}
      >
        <MinusIcon />
      </button>
      <output className="stepper__value num" aria-live="polite">
        {value}
      </output>
      <button
        type="button"
        className="stepper__button"
        aria-label={increaseLabel}
        title={increaseLabel}
        disabled={value >= max}
        onClick={() => onChange(Math.min(max, value + 1))}
      >
        <PlusIcon />
      </button>
    </div>
  );
}
