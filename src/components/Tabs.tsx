import { useRef, type KeyboardEvent } from "react";

export interface TabItem<T extends string> {
  id: T;
  label: string;
  count: number;
}

interface TabsProps<T extends string> {
  label: string;
  items: TabItem<T>[];
  selected: T;
  onSelect: (id: T) => void;
  /** Id of the element that the tabs control. */
  controls: string;
}

export function Tabs<T extends string>({ label, items, selected, onSelect, controls }: TabsProps<T>) {
  const refs = useRef<(HTMLButtonElement | null)[]>([]);

  function onKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    const current = items.findIndex((item) => item.id === selected);
    let next = -1;
    if (event.key === "ArrowRight") next = (current + 1) % items.length;
    else if (event.key === "ArrowLeft") next = (current - 1 + items.length) % items.length;
    else if (event.key === "Home") next = 0;
    else if (event.key === "End") next = items.length - 1;
    if (next === -1) return;
    event.preventDefault();
    onSelect(items[next].id);
    refs.current[next]?.focus();
  }

  return (
    <div className="tabs" role="tablist" aria-label={label} onKeyDown={onKeyDown}>
      {items.map((item, index) => {
        const isSelected = item.id === selected;
        return (
          <button
            key={item.id}
            ref={(el) => {
              refs.current[index] = el;
            }}
            type="button"
            role="tab"
            className={isSelected ? "tab tab--selected" : "tab"}
            aria-selected={isSelected}
            aria-controls={controls}
            tabIndex={isSelected ? 0 : -1}
            onClick={() => onSelect(item.id)}
          >
            {item.label}
            <span className="tab__count num">{item.count}</span>
          </button>
        );
      })}
    </div>
  );
}
