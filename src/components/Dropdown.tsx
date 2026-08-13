import { useEffect, useId, useRef, useState } from "react";

export interface DropdownOption {
  value: string;
  label: string;
}

interface Props {
  value: string;
  options: DropdownOption[];
  disabled?: boolean;
  label: string;
  /** Which edge the menu lines up with; the footer picker opens to the left. */
  align?: "start" | "end";
  className?: string;
  onChange: (value: string) => void;
}

/**
 * A styled replacement for `<select>`.
 *
 * The native drop-down list is painted by the platform, not by our CSS:
 * Chromium only honours a dark palette when the control has an opaque dark
 * background, and WebKitGTK ignores option colours entirely and follows the
 * GTK theme — so on a Linux box with a light theme the menu came out white in
 * the middle of a dark app. Drawing it ourselves is the only way to be sure.
 */
export function Dropdown({
  value,
  options,
  disabled,
  label,
  align = "start",
  className,
  onChange,
}: Props) {
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const root = useRef<HTMLDivElement>(null);
  const listId = useId();

  const selected = options.find((option) => option.value === value);

  useEffect(() => {
    if (!open) return;

    const onPointerDown = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) setOpen(false);
    };
    // Closing on scroll beats repositioning a menu that is anchored to a row.
    const onScroll = () => setOpen(false);

    document.addEventListener("pointerdown", onPointerDown);
    window.addEventListener("scroll", onScroll, true);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      window.removeEventListener("scroll", onScroll, true);
    };
  }, [open]);

  const commit = (next: string) => {
    onChange(next);
    setOpen(false);
  };

  const onKeyDown = (event: React.KeyboardEvent) => {
    if (disabled) return;

    if (!open) {
      if (["Enter", " ", "ArrowDown", "ArrowUp"].includes(event.key)) {
        event.preventDefault();
        setActive(Math.max(0, options.findIndex((option) => option.value === value)));
        setOpen(true);
      }
      return;
    }

    switch (event.key) {
      case "Escape":
        event.preventDefault();
        setOpen(false);
        break;
      case "ArrowDown":
        event.preventDefault();
        setActive((index) => (index + 1) % options.length);
        break;
      case "ArrowUp":
        event.preventDefault();
        setActive((index) => (index - 1 + options.length) % options.length);
        break;
      case "Home":
        event.preventDefault();
        setActive(0);
        break;
      case "End":
        event.preventDefault();
        setActive(options.length - 1);
        break;
      case "Enter":
      case " ":
        event.preventDefault();
        commit(options[active].value);
        break;
    }
  };

  return (
    <div className={`dropdown${className ? ` ${className}` : ""}`} ref={root}>
      <button
        type="button"
        className="dropdown-trigger"
        disabled={disabled}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={open ? listId : undefined}
        aria-label={label}
        onClick={() => {
          setActive(Math.max(0, options.findIndex((option) => option.value === value)));
          setOpen((wasOpen) => !wasOpen);
        }}
        onKeyDown={onKeyDown}
      >
        <span className="dropdown-value">{selected?.label ?? value}</span>
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" aria-hidden>
          <path
            d="m6 9 6 6 6-6"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </svg>
      </button>

      {open && (
        <ul className={`dropdown-menu is-${align}`} id={listId} role="listbox" aria-label={label}>
          {options.map((option, index) => (
            <li
              key={option.value}
              role="option"
              aria-selected={option.value === value}
              className={index === active ? "is-active" : undefined}
              onPointerEnter={() => setActive(index)}
              onClick={() => commit(option.value)}
            >
              {option.label}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
