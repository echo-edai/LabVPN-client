import { Search, Stethoscope } from "lucide-react";
import { useState } from "react";

import { Badge, Button, Card, CardHeader, Row, TextInput, type Tone } from "@/components/ui";
import { GROUP } from "@/lib/constants";
import type { DiagnosticsResult } from "@/lib/types";
import { useAppStore } from "@/stores/app";

function outboundTone(outbound: string): Tone {
  if (outbound === GROUP.campus) return "info";
  if (outbound === GROUP.proxy) return "brand";
  if (outbound === GROUP.ssh) return "warn";
  if (outbound === GROUP.adblock) return "err";
  if (outbound === "DIRECT") return "ok";
  return "muted";
}

export function DiagnosticsPanel({ className }: { className?: string }) {
  const t = useAppStore((state) => state.t);
  const diagnose = useAppStore((state) => state.diagnose);
  const busy = useAppStore((state) => state.busy === "diagnose");
  const [input, setInput] = useState("");
  const [result, setResult] = useState<DiagnosticsResult | null>(null);

  const run = async () => {
    if (!input.trim()) return;
    const outcome = await diagnose(input.trim());
    setResult(outcome);
  };

  return (
    <Card className={className}>
      <CardHeader
        title={t("merge.diagnostics")}
        subtitle={t("dashboard.quickDiagnose")}
        icon={<Stethoscope size={15} />}
      />
      <div className="flex gap-2">
        <TextInput
          value={input}
          placeholder={t("merge.diagnosePlaceholder")}
          onChange={(event) => setInput(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") void run();
          }}
        />
        <Button variant="primary" onClick={() => void run()} loading={busy} icon={<Search size={14} />}>
          {t("merge.diagnose")}
        </Button>
      </div>
      {result ? (
        <div className="mt-3 rounded-xl border border-line bg-card px-3 py-2.5">
          <div className="flex items-center justify-between gap-3">
            <div className="min-w-0">
              <div className="truncate text-[12.5px] font-medium text-ink">{result.input}</div>
              <div className="mt-0.5 truncate font-mono text-[10.5px] text-ink-muted" title={result.rule}>
                {result.rule}
              </div>
            </div>
            <Badge tone={outboundTone(result.outbound)}>{result.outbound}</Badge>
          </div>
          {result.notes.length > 0 ? (
            <div className="mt-2 space-y-0.5 border-t border-line pt-2">
              {result.notes.map((note, index) => (
                <div key={index} className="text-[10.5px] leading-4 text-ink-muted">
                  · {note}
                </div>
              ))}
            </div>
          ) : null}
        </div>
      ) : (
        <Row label={t("merge.notes")} value={t("merge.diagnosePlaceholder")} />
      )}
    </Card>
  );
}
