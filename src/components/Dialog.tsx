import {
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  type KeyboardEvent,
  type ReactNode,
  type RefObject,
} from "react";
import { IconButton } from "./IconButton";
import { CloseIcon } from "./icons";

const FOCUSABLE =
  'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

interface DialogProps {
  title: string;
  onClose: () => void;
  children: ReactNode;
  footer?: ReactNode;
  size?: "sm" | "md" | "lg";
  /** Element to focus when the dialog opens. Default: the first control in the body. */
  initialFocusRef?: RefObject<HTMLElement | null>;
}

/** Modal dialog. Esc and the Close button call onClose. Focus goes back to the opener. */
export function Dialog({ title, onClose, children, footer, size = "md", initialFocusRef }: DialogProps) {
  const ref = useRef<HTMLDialogElement>(null);
  const titleId = useId();
  const onCloseRef = useRef(onClose);
  const initialFocus = useRef(initialFocusRef);
  const unmounted = useRef(false);

  useEffect(() => {
    onCloseRef.current = onClose;
  });

  useLayoutEffect(() => {
    const dialog = ref.current;
    if (!dialog) return;
    unmounted.current = false;
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    if (!dialog.open) dialog.showModal();
    const target =
      initialFocus.current?.current ??
      dialog.querySelector<HTMLElement>(`.dialog__body :is(${FOCUSABLE})`) ??
      dialog.querySelector<HTMLElement>(FOCUSABLE);
    target?.focus();
    return () => {
      unmounted.current = true;
      if (dialog.open) dialog.close();
      if (opener?.isConnected) opener.focus();
    };
  }, []);

  function onKeyDown(event: KeyboardEvent<HTMLDialogElement>) {
    if (event.key !== "Tab" || !ref.current) return;
    const focusable = [...ref.current.querySelectorAll<HTMLElement>(FOCUSABLE)].filter(
      (el) => el.offsetParent !== null,
    );
    if (focusable.length === 0) return;
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  }

  return (
    <dialog
      ref={ref}
      className={`dialog dialog--${size}`}
      aria-labelledby={titleId}
      onKeyDown={onKeyDown}
      onCancel={(event) => {
        event.preventDefault();
        onCloseRef.current();
      }}
      onClose={(event) => {
        // The browser can close a modal dialog by force. Ignore the close that our cleanup causes.
        if (!unmounted.current && !event.currentTarget.open) onCloseRef.current();
      }}
    >
      <header className="dialog__header">
        <h2 className="dialog__title" id={titleId}>
          {title}
        </h2>
        <IconButton label="Close" icon={<CloseIcon />} onClick={() => onCloseRef.current()} />
      </header>
      <div className="dialog__body">{children}</div>
      {footer && <footer className="dialog__footer">{footer}</footer>}
    </dialog>
  );
}
