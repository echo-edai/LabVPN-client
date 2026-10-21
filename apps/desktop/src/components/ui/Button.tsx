import { Loader2 } from "lucide-react";
import type { ButtonHTMLAttributes, ReactNode } from "react";

import { cn } from "@/lib/cn";

type Variant = "primary" | "secondary" | "ghost" | "danger" | "success";
type Size = "sm" | "md" | "lg";

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant;
  size?: Size;
  loading?: boolean;
  icon?: ReactNode;
  block?: boolean;
}

const variants: Record<Variant, string> = {
  primary:
    "text-white bg-gradient-to-br from-[var(--brand)] to-[var(--brand-strong)] shadow-[0_10px_30px_-12px_rgba(37,99,235,0.7)] hover:brightness-110 active:brightness-95",
  secondary:
    "border border-line bg-card text-ink hover:bg-card-strong hover:border-line-strong",
  ghost: "text-ink-soft hover:text-ink hover:bg-card",
  danger: "text-white bg-err/90 hover:bg-err shadow-[0_10px_30px_-14px_rgba(244,63,94,0.6)]",
  success:
    "text-white bg-emerald-500/90 hover:bg-emerald-500 shadow-[0_10px_30px_-14px_rgba(16,185,129,0.6)]",
};

const sizes: Record<Size, string> = {
  sm: "h-8 px-3 text-xs rounded-lg gap-1.5",
  md: "h-10 px-4 text-[13px] rounded-xl gap-2",
  lg: "h-12 px-6 text-sm rounded-xl gap-2",
};

export function Button({
  variant = "secondary",
  size = "md",
  loading = false,
  icon,
  block = false,
  className,
  children,
  disabled,
  ...rest
}: ButtonProps) {
  return (
    <button
      className={cn(
        "inline-flex select-none items-center justify-center font-medium transition-all duration-150 disabled:cursor-not-allowed disabled:opacity-50",
        variants[variant],
        sizes[size],
        block && "w-full",
        className,
      )}
      disabled={disabled || loading}
      {...rest}
    >
      {loading ? <Loader2 size={size === "sm" ? 13 : 15} className="animate-spin" /> : icon}
      {children}
    </button>
  );
}

export interface IconButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  label: string;
  children: ReactNode;
}

export function IconButton({ label, children, className, ...rest }: IconButtonProps) {
  return (
    <button
      aria-label={label}
      title={label}
      className={cn(
        "inline-flex h-8 w-8 items-center justify-center rounded-lg border border-line bg-card text-ink-soft transition hover:border-line-strong hover:text-ink",
        className,
      )}
      {...rest}
    >
      {children}
    </button>
  );
}
