import {
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactNode,
} from "react";
import { IconButton } from "./IconButton";
import { MoreIcon } from "./icons";

export interface MenuItem {
  label: string;
  icon?: ReactNode;
  onSelect: () => void;
  danger?: boolean;
  /** Draws a line above the item. */
  separated?: boolean;
}

interface MenuProps {
  label: string;
  items: MenuItem[];
}

interface Position {
  top: number;
  left: number;
}

const GAP = 4;

/** A "More actions" button with a popup menu. Arrow keys move, Esc closes. */
export function Menu({ label, items }: MenuProps) {
  const [open, setOpen] = useState(false);
  const [position, setPosition] = useState<Position | null>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const menuId = useId();

  function close(returnFocus: boolean) {
    setOpen(false);
    setPosition(null);
    if (returnFocus) triggerRef.current?.focus();
  }

  useLayoutEffect(() => {
    if (!open || !triggerRef.current || !menuRef.current) return;
    const trigger = triggerRef.current.getBoundingClientRect();
    const menu = menuRef.current.getBoundingClientRect();
    const below = trigger.bottom + GAP;
    const top =
      below + menu.height > window.innerHeight ? Math.max(GAP, trigger.top - GAP - menu.height) : below;
    const left = Math.max(GAP, trigger.right - menu.width);
    setPosition({ top, left });
  }, [open]);

  useLayoutEffect(() => {
    if (position) menuRef.current?.querySelector<HTMLElement>('[role="menuitem"]')?.focus();
  }, [position]);

  useEffect(() => {
    if (!open) return;
    function onPointerDown(event: PointerEvent) {
      const target = event.target as Node;
      if (menuRef.current?.contains(target) || triggerRef.current?.contains(target)) return;
      close(false);
    }
    function onViewportChange() {
      close(false);
    }
    document.addEventListener("pointerdown", onPointerDown);
    window.addEventListener("resize", onViewportChange);
    window.addEventListener("scroll", onViewportChange, true);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      window.removeEventListener("resize", onViewportChange);
      window.removeEventListener("scroll", onViewportChange, true);
    };
  }, [open]);

  function onMenuKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    const entries = [...(menuRef.current?.querySelectorAll<HTMLElement>('[role="menuitem"]') ?? [])];
    const index = entries.indexOf(document.activeElement as HTMLElement);
    let next = -1;
    if (event.key === "ArrowDown") next = (index + 1) % entries.length;
    else if (event.key === "ArrowUp") next = (index - 1 + entries.length) % entries.length;
    else if (event.key === "Home") next = 0;
    else if (event.key === "End") next = entries.length - 1;
    else if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      close(true);
      return;
    } else if (event.key === "Tab") {
      close(false);
      return;
    }
    if (next === -1) return;
    event.preventDefault();
    entries[next]?.focus();
  }

  function onTriggerKeyDown(event: KeyboardEvent<HTMLButtonElement>) {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      setOpen(true);
    }
  }

  return (
    <>
      <IconButton
        ref={triggerRef}
        label={label}
        icon={<MoreIcon />}
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={open ? menuId : undefined}
        onClick={() => (open ? close(true) : setOpen(true))}
        onKeyDown={onTriggerKeyDown}
      />
      {open && (
        <div
          ref={menuRef}
          id={menuId}
          role="menu"
          aria-label={label}
          className="menu"
          style={
            position
              ? { top: position.top, left: position.left }
              : { top: 0, left: 0, visibility: "hidden" }
          }
          onKeyDown={onMenuKeyDown}
        >
          {items.map((item) => (
            <button
              key={item.label}
              type="button"
              role="menuitem"
              tabIndex={-1}
              className={[
                "menu__item",
                item.danger ? "menu__item--danger" : "",
                item.separated ? "menu__item--separated" : "",
              ].join(" ")}
              onClick={() => {
                close(true);
                item.onSelect();
              }}
            >
              {item.icon}
              {item.label}
            </button>
          ))}
        </div>
      )}
    </>
  );
}
