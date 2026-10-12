// Small building blocks used all over the app.

import { X } from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type ReactNode, type RefObject } from "react";
import { createPortal } from "react-dom";
import { formatNumber } from "../util";

type ButtonProps = React.ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: "default" | "primary" | "ghost" | "danger";
  size?: "sm" | "md";
  icon?: ReactNode;
  active?: boolean;
};

export function Button({ variant = "default", size = "md", icon, active, className, children, ...rest }: ButtonProps) {
  return (
    <button
      type="button"
      className={`btn btn--${variant} btn--${size} ${active ? "is-active" : ""} ${children == null ? "btn--icon" : ""} ${className ?? ""}`}
      {...rest}
    >
      {icon}
      {children != null && <span>{children}</span>}
    </button>
  );
}

export function Field({ label, hint, children, row }: { label?: ReactNode; hint?: ReactNode; children: ReactNode; row?: boolean }) {
  return (
    <div className={`field ${row ? "field--row" : ""}`}>
      {label && <div className="field__label">{label}</div>}
      <div className="field__control">{children}</div>
      {hint && <div className="field__hint">{hint}</div>}
    </div>
  );
}

interface SliderProps {
  label: ReactNode;
  value: number;
  min: number;
  max: number;
  step?: number;
  onChange: (v: number) => void;
  format?: (v: number) => string;
  /** Double-click the slider to return to this value. */
  resetTo?: number;
  hint?: ReactNode;
}

export function Slider({ label, value, min, max, step = 0.01, onChange, format, resetTo, hint }: SliderProps) {
  const pct = max > min ? ((value - min) / (max - min)) * 100 : 0;
  return (
    <label className="slider">
      <div className="slider__head">
        <span>{label}</span>
        <output>{format ? format(value) : formatNumber(value, step)}</output>
      </div>
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        style={{ "--fill": `${Math.min(100, Math.max(0, pct))}%` } as CSSProperties}
        onChange={(e) => onChange(Number(e.target.value))}
        onDoubleClick={() => resetTo != null && onChange(resetTo)}
        title={resetTo != null ? "Double-click to reset" : undefined}
      />
      {hint && <div className="field__hint">{hint}</div>}
    </label>
  );
}

export function Toggle({ label, checked, onChange, hint }: { label?: ReactNode; checked: boolean; onChange: (v: boolean) => void; hint?: ReactNode }) {
  return (
    <label className="toggle">
      <span className="toggle__text">
        {label && <span>{label}</span>}
        {hint && <span className="field__hint">{hint}</span>}
      </span>
      <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} />
      <span className="toggle__track" aria-hidden />
    </label>
  );
}

interface SelectProps<T extends string | number> {
  value: T;
  options: { value: T; label: string }[];
  onChange: (v: T) => void;
  disabled?: boolean;
}

export function Select<T extends string | number>({ value, options, onChange, disabled }: SelectProps<T>) {
  return (
    <select
      className="select"
      value={String(value)}
      disabled={disabled}
      onChange={(e) => {
        const picked = options.find((o) => String(o.value) === e.target.value);
        if (picked) onChange(picked.value);
      }}
    >
      {options.map((o) => (
        <option key={String(o.value)} value={String(o.value)}>
          {o.label}
        </option>
      ))}
    </select>
  );
}

interface SegmentedProps<T extends string> {
  value: T;
  options: { value: T; label: ReactNode; title?: string }[];
  onChange: (v: T) => void;
  stretch?: boolean;
}

export function Segmented<T extends string>({ value, options, onChange, stretch }: SegmentedProps<T>) {
  return (
    <div className={`segmented ${stretch ? "segmented--stretch" : ""}`} role="tablist">
      {options.map((o) => (
        <button
          type="button"
          key={o.value}
          role="tab"
          aria-selected={o.value === value}
          className={o.value === value ? "is-active" : ""}
          title={o.title}
          onClick={() => onChange(o.value)}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

export function TextInput({ value, onChange, placeholder, mono }: { value: string; onChange: (v: string) => void; placeholder?: string; mono?: boolean }) {
  return (
    <input
      className={`input ${mono ? "input--mono" : ""}`}
      type="text"
      value={value}
      placeholder={placeholder}
      spellCheck={false}
      onChange={(e) => onChange(e.target.value)}
    />
  );
}

interface NumberInputProps {
  value: number;
  onChange: (v: number) => void;
  min?: number;
  max?: number;
  step?: number;
  suffix?: string;
}

/** Lets the user type freely and only commits valid numbers. */
export function NumberInput({ value, onChange, min = -Infinity, max = Infinity, step = 1, suffix }: NumberInputProps) {
  const [text, setText] = useState(String(value));
  const focused = useRef(false);
  useEffect(() => {
    if (!focused.current) setText(String(value));
  }, [value]);
  const commit = (raw: string) => {
    const v = Number(raw);
    if (raw.trim() !== "" && Number.isFinite(v)) onChange(Math.min(max, Math.max(min, v)));
  };
  return (
    <div className="number">
      <input
        className="input"
        type="number"
        value={text}
        min={Number.isFinite(min) ? min : undefined}
        max={Number.isFinite(max) ? max : undefined}
        step={step}
        onFocus={() => (focused.current = true)}
        onBlur={() => {
          focused.current = false;
          setText(String(value));
        }}
        onChange={(e) => {
          setText(e.target.value);
          commit(e.target.value);
        }}
      />
      {suffix && <span className="number__suffix">{suffix}</span>}
    </div>
  );
}

interface ModalProps {
  title: ReactNode;
  subtitle?: ReactNode;
  onClose: () => void;
  children: ReactNode;
  footer?: ReactNode;
  width?: number;
}

export function Modal({ title, subtitle, onClose, children, footer, width = 720 }: ModalProps) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);
  return createPortal(
    <div className="modal-backdrop" onPointerDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="modal" style={{ width }} role="dialog" aria-modal>
        <header className="modal__head">
          <div>
            <h2>{title}</h2>
            {subtitle && <p>{subtitle}</p>}
          </div>
          <Button variant="ghost" icon={<X size={18} />} onClick={onClose} aria-label="Close" />
        </header>
        <div className="modal__body">{children}</div>
        {footer && <footer className="modal__foot">{footer}</footer>}
      </div>
    </div>,
    document.body,
  );
}

interface PopoverProps {
  anchor: RefObject<HTMLElement | null>;
  onClose: () => void;
  children: ReactNode;
  width?: number;
}

/** A floating panel pinned to an element, kept inside the window. */
export function Popover({ anchor, onClose, children, width = 260 }: PopoverProps) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ left: number; top: number } | null>(null);

  useLayoutEffect(() => {
    const place = () => {
      const a = anchor.current?.getBoundingClientRect();
      const el = ref.current;
      if (!a || !el) return;
      const h = el.offsetHeight;
      const left = Math.min(Math.max(8, a.left), window.innerWidth - width - 8);
      const below = a.bottom + 6;
      const top = below + h > window.innerHeight - 8 ? Math.max(8, a.top - h - 6) : below;
      setPos({ left, top });
    };
    place();
    window.addEventListener("resize", place);
    window.addEventListener("scroll", place, true);
    return () => {
      window.removeEventListener("resize", place);
      window.removeEventListener("scroll", place, true);
    };
  }, [anchor, width]);

  useEffect(() => {
    const onDown = (e: PointerEvent) => {
      const t = e.target as Node;
      if (ref.current?.contains(t) || anchor.current?.contains(t)) return;
      onClose();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        onClose();
      }
    };
    document.addEventListener("pointerdown", onDown, true);
    document.addEventListener("keydown", onKey, true);
    return () => {
      document.removeEventListener("pointerdown", onDown, true);
      document.removeEventListener("keydown", onKey, true);
    };
  }, [anchor, onClose]);

  return createPortal(
    <div ref={ref} className="popover" style={{ width, left: pos?.left ?? -9999, top: pos?.top ?? -9999 }}>
      {children}
    </div>,
    document.body,
  );
}

export function Empty({ icon, title, children, action }: { icon?: ReactNode; title: string; children?: ReactNode; action?: ReactNode }) {
  return (
    <div className="empty">
      {icon && <div className="empty__icon">{icon}</div>}
      <h3>{title}</h3>
      {children && <p>{children}</p>}
      {action}
    </div>
  );
}

export function Notice({ kind = "info", children }: { kind?: "info" | "warn" | "error"; children: ReactNode }) {
  return <div className={`notice notice--${kind}`}>{children}</div>;
}
