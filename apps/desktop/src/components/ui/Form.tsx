import { Eye, EyeOff } from "lucide-react";
import { useState, type InputHTMLAttributes, type ReactNode, type SelectHTMLAttributes, type TextareaHTMLAttributes } from "react";

import { cn } from "@/lib/cn";

export interface FieldProps {
  label?: ReactNode;
  hint?: ReactNode;
  error?: ReactNode;
  required?: boolean;
  optional?: boolean;
  optionalLabel?: string;
  children: ReactNode;
  className?: string;
}

export function Field({ label, hint, error, required, optional, optionalLabel, children, className }: FieldProps) {
  return (
    <label className={cn("block", className)}>
      {label ? (
        <span className="label flex items-center gap-1.5">
          {label}
          {required ? <span className="text-err">*</span> : null}
          {optional ? <span className="font-normal text-ink-muted">（{optionalLabel ?? "可选"}）</span> : null}
        </span>
      ) : null}
      {children}
      {error ? <span className="mt-1 block text-[11.5px] text-err">{error}</span> : null}
      {!error && hint ? <span className="hint mt-1 block">{hint}</span> : null}
    </label>
  );
}

export interface TextInputProps extends InputHTMLAttributes<HTMLInputElement> {
  invalid?: boolean;
}

export function TextInput({ className, invalid, ...rest }: TextInputProps) {
  return <input className={cn("input", invalid && "border-err focus:border-err", className)} {...rest} />;
}

export function PasswordInput({ className, ...rest }: TextInputProps) {
  const [visible, setVisible] = useState(false);
  return (
    <div className="relative">
      <input
        type={visible ? "text" : "password"}
        className={cn("input pr-10", className)}
        autoComplete="off"
        {...rest}
      />
      <button
        type="button"
        aria-label={visible ? "隐藏密码" : "显示密码"}
        onClick={() => setVisible((value) => !value)}
        className="absolute right-2 top-1/2 -translate-y-1/2 rounded-md p-1.5 text-ink-muted transition hover:bg-card hover:text-ink"
      >
        {visible ? <EyeOff size={14} /> : <Eye size={14} />}
      </button>
    </div>
  );
}

export function Textarea({ className, ...rest }: TextareaHTMLAttributes<HTMLTextAreaElement>) {
  return <textarea className={cn("input min-h-24 resize-y font-mono text-[12px] leading-5", className)} {...rest} />;
}

export interface SelectOption {
  value: string;
  label: string;
}

export interface SelectProps extends SelectHTMLAttributes<HTMLSelectElement> {
  options: SelectOption[];
}

export function Select({ options, className, ...rest }: SelectProps) {
  return (
    <select className={cn("input appearance-none pr-8", className)} {...rest}>
      {options.map((option) => (
        <option key={option.value} value={option.value} className="bg-[var(--bg-elev)] text-ink">
          {option.label}
        </option>
      ))}
    </select>
  );
}

export function Switch({
  checked,
  onChange,
  disabled,
  label,
  description,
}: {
  checked: boolean;
  onChange: (checked: boolean) => void;
  disabled?: boolean;
  label?: ReactNode;
  description?: ReactNode;
}) {
  return (
    <div className="flex items-start justify-between gap-4">
      {label || description ? (
        <div className="min-w-0">
          {label ? <div className="text-[13px] font-medium text-ink">{label}</div> : null}
          {description ? <div className="hint mt-0.5">{description}</div> : null}
        </div>
      ) : null}
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        disabled={disabled}
        onClick={() => onChange(!checked)}
        className={cn(
          "relative h-6 w-10 shrink-0 rounded-full border transition-colors duration-200",
          checked
            ? "border-transparent bg-gradient-to-r from-[var(--brand)] to-[var(--accent)]"
            : "border-line bg-card-strong",
          disabled && "cursor-not-allowed opacity-50",
        )}
      >
        <span
          className={cn(
            "absolute top-0.5 h-[18px] w-[18px] rounded-full bg-white shadow transition-all duration-200",
            checked ? "left-[20px]" : "left-0.5",
          )}
        />
      </button>
    </div>
  );
}

export interface SegmentedOption<T extends string> {
  value: T;
  label: ReactNode;
}

export function Segmented<T extends string>({
  value,
  options,
  onChange,
  className,
  size = "md",
}: {
  value: T;
  options: SegmentedOption<T>[];
  onChange: (value: T) => void;
  className?: string;
  size?: "sm" | "md";
}) {
  return (
    <div className={cn("inline-flex rounded-xl border border-line bg-card p-0.5", className)}>
      {options.map((option) => (
        <button
          key={option.value}
          type="button"
          onClick={() => onChange(option.value)}
          className={cn(
            "rounded-[10px] font-medium transition-all",
            size === "sm" ? "px-2.5 py-1 text-[11.5px]" : "px-3 py-1.5 text-xs",
            value === option.value
              ? "bg-card-strong text-ink shadow-sm"
              : "text-ink-muted hover:text-ink",
          )}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}
