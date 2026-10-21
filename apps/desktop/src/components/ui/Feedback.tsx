import { AlertTriangle, CheckCircle2, Info, Loader2, X, XCircle } from "lucide-react";
import type { ReactNode } from "react";

import { cn } from "@/lib/cn";
import { useAppStore } from "@/stores/app";

export type Tone = "ok" | "warn" | "err" | "info" | "brand" | "muted";

const toneText: Record<Tone, string> = {
  ok: "text-[var(--ok)]",
  warn: "text-[var(--warn)]",
  err: "text-[var(--err)]",
  info: "text-[var(--accent)]",
  brand: "text-brand",
  muted: "text-ink-muted",
};

const toneBg: Record<Tone, string> = {
  ok: "bg-[var(--ok)]/12 border-[var(--ok)]/25",
  warn: "bg-[var(--warn)]/12 border-[var(--warn)]/25",
  err: "bg-[var(--err)]/12 border-[var(--err)]/25",
  info: "bg-[var(--accent)]/12 border-[var(--accent)]/25",
  brand: "bg-[var(--brand)]/12 border-[var(--brand)]/25",
  muted: "bg-card border-line",
};

export function StatusDot({ tone = "muted", pulse = false }: { tone?: Tone; pulse?: boolean }) {
  return (
    <span className="relative inline-flex h-2 w-2 shrink-0">
      {pulse ? (
        <span className={cn("absolute inline-flex h-full w-full animate-ping rounded-full opacity-60", toneText[tone].replace("text-", "bg-"))} />
      ) : null}
      <span className={cn("relative inline-flex h-2 w-2 rounded-full", toneText[tone].replace("text-", "bg-"))} />
    </span>
  );
}

export function Badge({
  tone = "muted",
  icon,
  children,
  className,
}: {
  tone?: Tone;
  icon?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 rounded-full border px-2 py-0.5 text-[11px] font-medium",
        toneBg[tone],
        toneText[tone],
        className,
      )}
    >
      {icon}
      {children}
    </span>
  );
}

export function Spinner({ size = 14 }: { size?: number }) {
  return <Loader2 size={size} className="animate-spin text-ink-muted" />;
}

export function EmptyState({ icon, title, hint }: { icon?: ReactNode; title: ReactNode; hint?: ReactNode }) {
  return (
    <div className="flex flex-col items-center justify-center gap-2 py-10 text-center">
      {icon ? <div className="text-ink-muted">{icon}</div> : null}
      <div className="text-[13px] font-medium text-ink-soft">{title}</div>
      {hint ? <div className="hint max-w-xs">{hint}</div> : null}
    </div>
  );
}

export function Progress({ ratio, tone = "brand" }: { ratio: number; tone?: Tone }) {
  const percent = Math.min(100, Math.max(0, ratio * 100));
  return (
    <div className="h-1.5 w-full overflow-hidden rounded-full bg-card-strong">
      <div
        className={cn(
          "h-full rounded-full transition-all duration-500",
          tone === "brand" && "bg-gradient-to-r from-[var(--brand)] to-[var(--accent)]",
          tone === "warn" && "bg-[var(--warn)]",
          tone === "err" && "bg-[var(--err)]",
          tone === "ok" && "bg-[var(--ok)]",
        )}
        style={{ width: `${percent}%` }}
      />
    </div>
  );
}

const toastIcon = {
  success: <CheckCircle2 size={16} className="text-[var(--ok)]" />,
  error: <XCircle size={16} className="text-[var(--err)]" />,
  warn: <AlertTriangle size={16} className="text-[var(--warn)]" />,
  info: <Info size={16} className="text-[var(--accent)]" />,
};

export function ToastHost() {
  const toasts = useAppStore((state) => state.toasts);
  const dismiss = useAppStore((state) => state.dismissToast);
  return (
    <div className="pointer-events-none fixed bottom-5 right-5 z-50 flex w-[340px] flex-col gap-2">
      {toasts.map((toast) => (
        <div
          key={toast.id}
          className="animate-fade-up pointer-events-auto flex items-start gap-2.5 rounded-xl border border-line bg-[var(--bg-elev)]/95 p-3 shadow-2xl backdrop-blur"
        >
          <div className="mt-0.5">{toastIcon[toast.kind]}</div>
          <div className="min-w-0 flex-1">
            <div className="text-[12.5px] font-semibold text-ink">{toast.title}</div>
            {toast.message ? (
              <div className="mt-0.5 break-words text-[11.5px] leading-4 text-ink-muted">{toast.message}</div>
            ) : null}
          </div>
          <button
            type="button"
            aria-label="关闭"
            onClick={() => dismiss(toast.id)}
            className="rounded p-0.5 text-ink-muted transition hover:text-ink"
          >
            <X size={13} />
          </button>
        </div>
      ))}
    </div>
  );
}

export function Modal({
  open,
  title,
  children,
  footer,
  onClose,
}: {
  open: boolean;
  title: ReactNode;
  children: ReactNode;
  footer?: ReactNode;
  onClose: () => void;
}) {
  if (!open) return null;
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-6">
      <div className="absolute inset-0 bg-black/55 backdrop-blur-sm" onClick={onClose} />
      <div className="animate-fade-up relative w-full max-w-md rounded-2xl border border-line bg-[var(--bg-elev)] p-5 shadow-2xl">
        <div className="mb-3 text-sm font-semibold text-ink">{title}</div>
        <div className="text-[12.5px] leading-5 text-ink-soft">{children}</div>
        {footer ? <div className="mt-5 flex justify-end gap-2">{footer}</div> : null}
      </div>
    </div>
  );
}
