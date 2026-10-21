import type { HTMLAttributes, ReactNode } from "react";

import { cn } from "@/lib/cn";

export interface CardProps extends HTMLAttributes<HTMLDivElement> {
  interactive?: boolean;
  padded?: boolean;
}

export function Card({ interactive = false, padded = true, className, children, ...rest }: CardProps) {
  return (
    <div
      className={cn("card relative", padded && "p-5", interactive && "card-interactive", className)}
      {...rest}
    >
      {children}
    </div>
  );
}

export interface CardHeaderProps {
  title: ReactNode;
  subtitle?: ReactNode;
  icon?: ReactNode;
  actions?: ReactNode;
  className?: string;
}

export function CardHeader({ title, subtitle, icon, actions, className }: CardHeaderProps) {
  return (
    <div className={cn("mb-4 flex items-start justify-between gap-3", className)}>
      <div className="flex min-w-0 items-start gap-3">
        {icon ? (
          <div className="mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded-lg border border-line bg-card text-brand">
            {icon}
          </div>
        ) : null}
        <div className="min-w-0">
          <div className="truncate text-[13.5px] font-semibold text-ink">{title}</div>
          {subtitle ? <div className="mt-0.5 text-xs text-ink-muted">{subtitle}</div> : null}
        </div>
      </div>
      {actions ? <div className="flex shrink-0 items-center gap-2">{actions}</div> : null}
    </div>
  );
}

export function SectionTitle({ children, hint }: { children: ReactNode; hint?: ReactNode }) {
  return (
    <div className="mb-2 flex items-center gap-2">
      <h3 className="text-xs font-semibold uppercase tracking-wider text-ink-muted">{children}</h3>
      {hint ? <span className="text-[11px] text-ink-muted">{hint}</span> : null}
    </div>
  );
}

export function Row({
  label,
  value,
  mono = false,
}: {
  label: ReactNode;
  value: ReactNode;
  mono?: boolean;
}) {
  return (
    <div className="flex items-center justify-between gap-3 py-1.5 text-[12.5px]">
      <span className="shrink-0 text-ink-muted">{label}</span>
      <span className={cn("min-w-0 truncate text-right text-ink-soft", mono && "font-mono text-[11.5px]")}>
        {value}
      </span>
    </div>
  );
}
