import { CloudDownload, FileUp, Route, Trash2 } from "lucide-react";
import { useRef, useState } from "react";

import { Badge, Button, Card, CardHeader, Field, Select } from "@/components/ui";
import { api, IpcError } from "@/lib/ipc";
import type { ImportedRuleProfile, RuleImportPreview, RuleOutlet } from "@/lib/types";
import { useAppStore } from "@/stores/app";

const OUTLETS: RuleOutlet[] = ["proxy", "direct", "reject", "campus", "ssh"];
const MAX_FILE_BYTES = 3 * 1024 * 1024;

export function RuleImportPanel({
  profile,
  onChange,
  disabled,
}: {
  profile: ImportedRuleProfile | null;
  onChange: (profile: ImportedRuleProfile | null) => void;
  disabled: boolean;
}) {
  const t = useAppStore((state) => state.t);
  const fileInput = useRef<HTMLInputElement>(null);
  const chooseButton = useRef<HTMLButtonElement>(null);
  const [defaultOutlet, setDefaultOutlet] = useState<RuleOutlet>("proxy");
  const [fetchPublic, setFetchPublic] = useState(false);
  const [loading, setLoading] = useState<"file" | "public" | null>(null);
  const [preview, setPreview] = useState<RuleImportPreview | null>(null);
  const [error, setError] = useState("");
  const locked = disabled || Boolean(loading);
  const outletLabel = (outlet: RuleOutlet | null) => t(`ruleImport.outlet.${outlet ?? "unknown"}`);
  const appliedCount = profile?.entries.reduce((count, entry) => count + entry.matchers.length, 0);

  const load = async (file: File | null) => {
    setLoading(file ? "file" : "public");
    setError("");
    setPreview(null);
    try {
      if (file && file.size > MAX_FILE_BYTES) {
        setError(t("ruleImport.tooLarge"));
        return;
      }
      const local = file ? { name: file.name, text: new TextDecoder("utf-8", { fatal: true }).decode(await file.arrayBuffer()) } : null;
      const result = await api.previewRuleImport(local, defaultOutlet, fetchPublic);
      setPreview(result);
    } catch (error) {
      if (error instanceof IpcError && error.code === "rule_import_download") {
        setError(t("ruleImport.downloadError"));
      } else if (error instanceof IpcError && error.code === "rule_import_too_large") {
        setError(t("ruleImport.tooLarge"));
      } else {
        setError(t("ruleImport.invalidFile"));
      }
    } finally {
      setLoading(null);
    }
  };

  const accept = () => {
    if (!preview?.profile) return;
    onChange(preview.profile);
    setPreview(null);
    chooseButton.current?.focus();
  };

  return (
    <Card>
      <CardHeader title={t("ruleImport.title")} icon={<Route size={15} />} />
      {profile ? (
        <div className="mb-3 flex items-center justify-between gap-2 rounded-lg border border-line px-3 py-2">
          <div className="min-w-0">
            <div className="break-all text-sm font-medium text-ink">{profile.name}</div>
            <div className="text-xs text-ink-muted">{t("ruleImport.profileCount", { count: appliedCount ?? 0 })}</div>
          </div>
          <Button size="sm" variant="ghost" disabled={locked} icon={<Trash2 size={13} />} onClick={() => onChange(null)}>
            {t("ruleImport.remove")}
          </Button>
        </div>
      ) : null}
      <div className="mb-3 grid grid-cols-1 items-center gap-2 sm:grid-cols-2">
        <Field label={t("ruleImport.defaultOutlet")}>
          <Select
            value={defaultOutlet}
            disabled={locked}
            options={OUTLETS.map((outlet) => ({ value: outlet, label: outletLabel(outlet) }))}
            onChange={(event) => setDefaultOutlet(event.target.value as RuleOutlet)}
          />
        </Field>
        <label className="flex items-center gap-2 text-xs text-ink-soft">
          <input type="checkbox" className="shrink-0 accent-[var(--brand)]" checked={fetchPublic} disabled={locked} onChange={(event) => setFetchPublic(event.target.checked)} />
          {t("ruleImport.fetchPublic")}
        </label>
      </div>
      <input
        type="file"
        ref={fileInput}
        hidden
        accept=".conf,.list,.txt,.rules,.yaml,.yml"
        onChange={(event) => {
          const file = event.target.files?.[0];
          event.target.value = "";
          if (file && !locked) void load(file);
        }}
      />
      <div className="flex flex-wrap gap-2" aria-busy={Boolean(loading)}>
        <Button ref={chooseButton} size="sm" variant="secondary" disabled={locked} loading={loading === "file"} icon={<FileUp size={13} />} onClick={() => fileInput.current?.click()}>
          {t("ruleImport.choose")}
        </Button>
        <Button size="sm" variant="ghost" disabled={locked} loading={loading === "public"} icon={<CloudDownload size={13} />} onClick={() => void load(null)}>
          {t("ruleImport.public")}
        </Button>
      </div>
      {error ? <p role="alert" className="mt-3 text-xs text-err">{error}</p> : null}
      {preview ? (
        <section className="mt-3 space-y-3 border-t border-line pt-3" aria-label={t("ruleImport.preview")}>
          <div role="status" className="flex flex-wrap items-center gap-2">
            <span className="text-sm font-medium text-ink">{t("ruleImport.preview")}</span>
            {preview.profile ? <span className="break-all text-xs text-ink-soft">{preview.profile.name}</span> : null}
            <Badge tone="brand">{t("ruleImport.summary", { rules: preview.ruleCount, sets: preview.setCount })}</Badge>
            {preview.profile?.fallback ? <Badge tone="muted">{t("ruleImport.fallback", { outlet: outletLabel(preview.profile.fallback) })}</Badge> : null}
          </div>
          {preview.skipped.length ? (
            <ul className="space-y-1 text-xs leading-5 text-[var(--warn)]">
              {preview.skipped.map((row) => (
                <li key={row.reason}>
                  {t(`ruleImport.issue.${row.reason}`, { count: row.count })}
                  {row.lines.length ? ` · ${t("ruleImport.positions", { lines: row.lines.join(", ") })}` : ""}
                </li>
              ))}
            </ul>
          ) : null}
          {preview.mappings.length || preview.samples.length ? (
            <details className="text-xs text-ink-soft">
              <summary className="cursor-pointer rounded-sm font-medium focus-visible:outline-2 focus-visible:outline-[var(--brand)]">{t("ruleImport.details")}</summary>
              <div className="scroll-slim mt-2 max-h-44 space-y-2 overflow-auto">
                <ul className="grid grid-cols-1 gap-1 sm:grid-cols-2">
                  {preview.mappings.map((mapping) => (
                    <li key={mapping.source} className="break-words">{mapping.source} → {outletLabel(mapping.outlet)}</li>
                  ))}
                </ul>
                {preview.samples.length ? <pre className="whitespace-pre-wrap break-all text-[11px] leading-5">{preview.samples.join("\n")}</pre> : null}
              </div>
            </details>
          ) : null}
          {!preview.profile ? <p className="text-xs text-ink-muted">{t("ruleImport.empty")}</p> : null}
          <div className="flex flex-wrap gap-2">
            <Button size="sm" disabled={locked || !preview.profile} onClick={accept}>{t(profile ? "ruleImport.replace" : "ruleImport.stage")}</Button>
            <Button size="sm" variant="ghost" onClick={() => { setPreview(null); chooseButton.current?.focus(); }}>{t("common.cancel")}</Button>
          </div>
        </section>
      ) : null}
    </Card>
  );
}
