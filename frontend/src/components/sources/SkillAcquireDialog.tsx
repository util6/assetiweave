import { AlertTriangle, CheckCircle2, DownloadCloud, ExternalLink, Search, X } from "lucide-react";
import { useEffect, useId, useRef, useState, type FormEvent, type ReactNode } from "react";
import { useI18n } from "../../i18n/I18nProvider";
import { acquireSkill, searchSkills } from "../../services/catalog";
import type { SkillAcquireResult, SkillSearchCandidate } from "../../types";
import { openExternalLink } from "../../utils/externalLinks";
import { DialogFrame } from "../foundation/DialogFrame";
import { EmptyState } from "../foundation/EmptyState";
import { Button } from "../ui/button";
import { Input } from "../ui/input";

export function SkillAcquireDialog({
  onAcquired,
  onClose,
  onNotifyError,
  open,
}: {
  onAcquired: () => Promise<void>;
  onClose: () => void;
  onNotifyError: (message: string) => void;
  open: boolean;
}) {
  const { t } = useI18n();
  const queryInputRef = useRef<HTMLInputElement>(null);
  const queryErrorId = useId();
  const urlErrorId = useId();
  const [query, setQuery] = useState("");
  const [candidates, setCandidates] = useState<SkillSearchCandidate[]>([]);
  const [searchWarnings, setSearchWarnings] = useState<string[]>([]);
  const [url, setUrl] = useState("");
  const [branch, setBranch] = useState("");
  const [path, setPath] = useState("");
  const [name, setName] = useState("");
  const [plan, setPlan] = useState<SkillAcquireResult | null>(null);
  const [busy, setBusy] = useState<"search" | "preview" | "import" | null>(null);
  const [queryError, setQueryError] = useState(false);
  const [urlError, setUrlError] = useState(false);

  useEffect(() => {
    if (!open) {
      return;
    }
    setQuery("");
    setCandidates([]);
    setSearchWarnings([]);
    setUrl("");
    setBranch("");
    setPath("");
    setName("");
    setPlan(null);
    setBusy(null);
    setQueryError(false);
    setUrlError(false);
    window.setTimeout(() => queryInputRef.current?.focus(), 0);
  }, [open]);

  useEffect(() => {
    if (!open) {
      return;
    }

    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape" && !busy) {
        onClose();
      }
    }

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [busy, onClose, open]);

  if (!open) {
    return null;
  }

  const disabled = Boolean(busy);

  async function handleSearch(event?: FormEvent<HTMLFormElement>) {
    event?.preventDefault();
    const trimmedQuery = query.trim();
    setQueryError(!trimmedQuery);
    if (!trimmedQuery) {
      return;
    }

    setBusy("search");
    try {
      const result = await searchSkills(trimmedQuery, 8);
      setCandidates(result.candidates);
      setSearchWarnings(result.warnings);
    } catch (error) {
      onNotifyError(errorMessage(error, t("skillAcquire.error.search")));
    } finally {
      setBusy(null);
    }
  }

  async function handlePreview(next?: Partial<{ branch: string; name: string; path: string; url: string }>) {
    const nextUrl = (next?.url ?? url).trim();
    setUrlError(!nextUrl);
    if (!nextUrl) {
      return;
    }

    const request = {
      branch: (next?.branch ?? branch).trim(),
      name: (next?.name ?? name).trim(),
      path: (next?.path ?? path).trim(),
      url: nextUrl,
    };
    setUrl(request.url);
    setBranch(request.branch);
    setPath(request.path);
    setName(request.name);

    setBusy("preview");
    try {
      setPlan(
        await acquireSkill({
          ...request,
          dryRun: true,
        }),
      );
    } catch (error) {
      setPlan(null);
      onNotifyError(errorMessage(error, t("skillAcquire.error.preview")));
    } finally {
      setBusy(null);
    }
  }

  async function handleImport() {
    const trimmedUrl = url.trim();
    setUrlError(!trimmedUrl);
    if (!trimmedUrl) {
      return;
    }

    setBusy("import");
    try {
      await acquireSkill({
        branch: branch.trim(),
        dryRun: false,
        name: name.trim(),
        path: path.trim(),
        url: trimmedUrl,
      });
      await onAcquired();
      onClose();
    } catch (error) {
      onNotifyError(errorMessage(error, t("skillAcquire.error.import")));
    } finally {
      setBusy(null);
    }
  }

  return (
    <DialogFrame
      className="flex max-h-full max-w-5xl flex-col"
      contentClassName="min-h-0 overflow-y-auto p-0"
      description={t("skillAcquire.dialog.description")}
      headerActions={
        <Button
          aria-label={t("skillAcquire.close")}
          disabled={disabled}
          onClick={onClose}
          size="icon"
          title={t("skillAcquire.close")}
          type="button"
          variant="ghost"
        >
          <X size={18} />
        </Button>
      }
      icon={<DownloadCloud size={18} />}
      iconClassName="border-status-create/25 bg-status-create/15 text-status-create"
      onBackdropClick={disabled ? undefined : onClose}
      overlayClassName="z-40 px-6 py-8"
      title={t("skillAcquire.dialog.title")}
    >
      <div className="grid gap-0 lg:grid-cols-[minmax(0,1fr)_minmax(20rem,0.85fr)]">
        <section className="min-w-0 border-b border-theme-card-border p-5 lg:border-b-0 lg:border-r">
          <form className="flex gap-2" onSubmit={(event) => void handleSearch(event)}>
            <Input
              aria-describedby={queryError ? queryErrorId : undefined}
              aria-invalid={queryError}
              className="min-w-0 flex-1"
              disabled={disabled}
              onChange={(event) => {
                setQuery(event.target.value);
                setQueryError(false);
              }}
              placeholder={t("skillAcquire.search.placeholder")}
              ref={queryInputRef}
              value={query}
            />
            <Button disabled={disabled} type="submit">
              <Search size={16} />
              {busy === "search" ? t("skillAcquire.search.searching") : t("skillAcquire.search.submit")}
            </Button>
          </form>
          {queryError && (
            <span className="mt-1.5 block text-body-sm text-status-remove" id={queryErrorId}>
              {t("skillAcquire.error.queryRequired")}
            </span>
          )}
          {searchWarnings.length > 0 && (
            <div className="mt-3 rounded-lg border border-status-conflict/30 bg-status-conflict/10 p-3 text-body-sm text-on-surface-variant">
              <div className="flex items-center gap-2 font-medium text-on-surface">
                <AlertTriangle size={15} />
                <span>{t("skillAcquire.warning.title")}</span>
              </div>
              <ul className="mt-2 grid gap-1">
                {searchWarnings.map((warning) => (
                  <li className="break-words" key={warning}>
                    {warning}
                  </li>
                ))}
              </ul>
            </div>
          )}

          <div className="mt-4 grid gap-2">
            {candidates.length === 0 ? (
              <EmptyState
                className="min-h-64"
                description={t("skillAcquire.empty.description")}
                icon={<Search size={19} />}
                title={t("skillAcquire.empty.title")}
              />
            ) : (
              candidates.map((candidate) => (
                <CandidateRow
                  busy={disabled}
                  candidate={candidate}
                  key={candidate.url}
                  onOpen={() => openExternalLink(candidate.url)}
                  onPreview={() =>
                    void handlePreview({
                      name: candidate.name.split("/").pop() ?? candidate.name,
                      url: candidate.url,
                    })
                  }
                />
              ))
            )}
          </div>
        </section>

        <section className="min-w-0 p-5">
          <div className="grid gap-3">
            <Field label={t("skillAcquire.field.url")} required>
              <Input
                aria-describedby={urlError ? urlErrorId : undefined}
                aria-invalid={urlError}
                disabled={disabled}
                onChange={(event) => {
                  setUrl(event.target.value);
                  setUrlError(false);
                  setPlan(null);
                }}
                placeholder={t("skillAcquire.field.urlPlaceholder")}
                value={url}
              />
              {urlError && (
                <span className="text-body-sm text-status-remove" id={urlErrorId}>
                  {t("skillAcquire.error.urlRequired")}
                </span>
              )}
            </Field>

            <div className="grid grid-cols-2 gap-3 max-[720px]:grid-cols-1">
              <Field label={t("skillAcquire.field.branch")}>
                <Input
                  disabled={disabled}
                  onChange={(event) => setBranch(event.target.value)}
                  placeholder={t("skillAcquire.field.branchPlaceholder")}
                  value={branch}
                />
              </Field>
              <Field label={t("skillAcquire.field.path")}>
                <Input
                  disabled={disabled}
                  onChange={(event) => setPath(event.target.value)}
                  placeholder={t("skillAcquire.field.pathPlaceholder")}
                  value={path}
                />
              </Field>
            </div>

            <Field label={t("skillAcquire.field.name")}>
              <Input
                disabled={disabled}
                onChange={(event) => setName(event.target.value)}
                placeholder={t("skillAcquire.field.namePlaceholder")}
                value={name}
              />
            </Field>

            <div className="flex justify-end gap-2">
              <Button disabled={disabled} onClick={() => void handlePreview()} type="button" variant="outline">
                <Search size={16} />
                {busy === "preview" ? t("skillAcquire.preview.loading") : t("skillAcquire.preview.submit")}
              </Button>
              <Button disabled={disabled || !plan} onClick={() => void handleImport()} type="button">
                <DownloadCloud size={16} />
                {busy === "import" ? t("skillAcquire.import.importing") : t("skillAcquire.import.submit")}
              </Button>
            </div>

            <div className="rounded-lg border border-status-conflict/30 bg-status-conflict/10 p-3 text-body-sm text-on-surface-variant">
              <div className="flex items-center gap-2 font-medium text-on-surface">
                <AlertTriangle size={15} />
                <span>{t("skillAcquire.security.title")}</span>
              </div>
              <p className="mt-1">{plan?.security_notice ?? t("skillAcquire.security.message")}</p>
            </div>

            <AcquirePlan plan={plan} />
          </div>
        </section>
      </div>
    </DialogFrame>
  );
}

function CandidateRow({
  busy,
  candidate,
  onOpen,
  onPreview,
}: {
  busy: boolean;
  candidate: SkillSearchCandidate;
  onOpen: () => void;
  onPreview: () => void;
}) {
  const { t } = useI18n();

  return (
    <article className="grid gap-2 rounded-lg border border-theme-control-border bg-theme-control/75 p-3">
      <div className="flex min-w-0 items-start justify-between gap-3">
        <div className="min-w-0">
          <h3 className="overflow-hidden text-ellipsis whitespace-nowrap font-mono text-body-sm font-semibold text-on-surface">
            {candidate.name}
          </h3>
          {candidate.description && (
            <p className="mt-1 line-clamp-2 text-body-sm text-on-surface-variant">{candidate.description}</p>
          )}
          {candidate.match_reason && (
            <p className="mt-1 rounded-md border border-theme-card-border bg-theme-card px-2 py-1 text-body-sm text-on-surface-variant">
              {candidate.match_reason}
            </p>
          )}
        </div>
        <div className="shrink-0 rounded-md border border-theme-card-border bg-theme-card px-2 py-0.5 font-mono text-body-sm text-primary">
          {candidate.stars ?? 0}
        </div>
      </div>
      <div className="flex flex-wrap justify-end gap-2">
        <Button disabled={busy} onClick={onOpen} size="sm" type="button" variant="ghost">
          <ExternalLink size={15} />
          {t("skillAcquire.candidate.open")}
        </Button>
        <Button disabled={busy} onClick={onPreview} size="sm" type="button" variant="outline">
          <Search size={15} />
          {t("skillAcquire.candidate.preview")}
        </Button>
      </div>
    </article>
  );
}

function AcquirePlan({ plan }: { plan: SkillAcquireResult | null }) {
  const { t } = useI18n();

  if (!plan) {
    return (
      <div className="rounded-lg border border-dashed border-theme-card-border bg-theme-card/40 p-4 text-body-sm text-on-surface-variant">
        {t("skillAcquire.preview.empty")}
      </div>
    );
  }

  return (
    <section className="grid gap-2 rounded-lg border border-theme-card-border bg-theme-card/65 p-3">
      <div className="flex items-center gap-2 text-label-caps uppercase text-outline">
        <CheckCircle2 size={15} />
        <span>{t("skillAcquire.preview.title")}</span>
      </div>
      <PlanRow label={t("skillAcquire.preview.name")} value={plan.name} />
      <PlanRow label={t("skillAcquire.preview.repo")} value={plan.repo_url} />
      {plan.branch && <PlanRow label={t("skillAcquire.preview.branch")} value={plan.branch} />}
      {plan.path && <PlanRow label={t("skillAcquire.preview.path")} value={plan.path} />}
      <PlanRow label={t("skillAcquire.preview.staging")} value={plan.staging_path} />
      <PlanRow label={t("skillAcquire.preview.skillPath")} value={plan.skill_path} />
    </section>
  );
}

function PlanRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="grid gap-1 rounded-md border border-theme-control-border bg-theme-control px-3 py-2">
      <div className="text-label-caps uppercase text-outline">{label}</div>
      <div className="break-all font-mono text-body-sm text-on-surface">{value}</div>
    </div>
  );
}

function Field({ children, label, required = false }: { children: ReactNode; label: string; required?: boolean }) {
  return (
    <label className="grid gap-1.5">
      <span className="text-body-sm font-medium text-on-surface-variant">
        {label}
        {required && <span className="text-status-remove"> *</span>}
      </span>
      {children}
    </label>
  );
}

function errorMessage(error: unknown, fallback: string) {
  return error instanceof Error ? error.message : fallback;
}
