import { Check, Copy } from "lucide-react";
import { useState } from "react";

import { cn } from "@/lib/cn";
import { useAppStore } from "@/stores/app";

export function CodeBlock({
  code,
  maxHeight = 360,
  className,
  language = "yaml",
}: {
  code: string;
  maxHeight?: number;
  className?: string;
  language?: string;
}) {
  const [copied, setCopied] = useState(false);
  const pushToast = useAppStore((state) => state.pushToast);
  const t = useAppStore((state) => state.t);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(code);
      setCopied(true);
      pushToast("success", t("toast.copied"));
      setTimeout(() => setCopied(false), 1500);
    } catch {
      pushToast("error", t("toast.failed"));
    }
  };

  return (
    <div className={cn("relative overflow-hidden rounded-xl border border-line bg-[#04070d]/80", className)}>
      <div className="flex items-center justify-between border-b border-line px-3 py-1.5">
        <span className="font-mono text-[10.5px] uppercase tracking-wider text-ink-muted">{language}</span>
        <button
          type="button"
          onClick={copy}
          className="inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-[11px] text-ink-muted transition hover:bg-card hover:text-ink"
        >
          {copied ? <Check size={11} /> : <Copy size={11} />}
          {copied ? "已复制" : "复制"}
        </button>
      </div>
      <pre
        className="scroll-slim p-3 font-mono text-[11.5px] leading-[1.65] text-[#c9d4ea]"
        style={{ maxHeight }}
      >
        {code}
      </pre>
    </div>
  );
}
