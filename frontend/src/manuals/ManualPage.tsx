import { useMemo, useState, type ReactNode } from "react";
import {
  AlertTriangle,
  ArrowLeft,
  BookOpen,
  CheckCircle2,
  ChevronDown,
  Lightbulb,
  ListChecks,
  Route,
  Search,
  Target,
} from "lucide-react";
import { PageHeader } from "../components/foundation/PageHeader";
import { useI18n } from "../i18n/I18nProvider";
import { getManualContent, getManualDocument } from "./registry";
import type { ManualSection } from "./types";

export function ManualPage({
  onBack,
  routeKey,
}: {
  onBack: () => void;
  routeKey: string;
}) {
  const { locale } = useI18n();
  const [query, setQuery] = useState("");
  const [expandedHeadings, setExpandedHeadings] = useState<Set<string>>(
    () =>
      new Set([
        "常用流程",
        "Common workflow",
        "创建和维护分组",
        "Create and maintain groups",
        "导入来源",
        "Import sources",
        "维护目标 App",
        "Maintain target apps",
        "同步和浏览",
        "Sync and browse",
        "来源和适配器",
        "Sources and adapters",
        "aICLI / assetiweave-cli",
        "使用方式",
        "How to use",
        "适配器状态",
        "Adapter state",
        "当前可用",
        "Available now",
      ]),
  );
  const document = getManualDocument(routeKey);
  const content = document
    ? getManualContent(document, locale)
    : {
        title: locale === "zh" ? "页面使用手册" : "Page Manual",
        subtitle: routeKey,
        overview: locale === "zh" ? "这个页面暂未配置手册内容。" : "No manual content is configured for this page yet.",
        sections: [],
      };
  const backLabel = locale === "zh" ? "返回页面" : "Back to page";
  const eyebrow = locale === "zh" ? "使用手册" : "Manual";
  const routeLabel = locale === "zh" ? "路由" : "Route";
  const searchLabel = locale === "zh" ? "搜索本页手册" : "Search this manual";
  const searchPlaceholder = locale === "zh" ? "搜索流程、状态、风险或操作..." : "Search workflow, state, risk, or actions...";
  const sectionLabel = locale === "zh" ? "章节" : "Sections";
  const stepLabel = locale === "zh" ? "条目" : "Items";
  const overviewLabel = locale === "zh" ? "页面概览" : "Overview";
  const noResultsLabel = locale === "zh" ? "没有匹配的手册内容。" : "No manual content matched your search.";
  const outcomesLabel = locale === "zh" ? "能帮你完成" : "What this helps with";
  const stepsBlockLabel = locale === "zh" ? "推荐操作步骤" : "Recommended steps";
  const cautionsLabel = locale === "zh" ? "注意事项" : "Watch points";
  const keywordsLabel = locale === "zh" ? "关键词" : "Keywords";
  const expandAllLabel = locale === "zh" ? "全部展开" : "Expand all";
  const collapseAllLabel = locale === "zh" ? "全部收起" : "Collapse all";
  const emptyBlockLabel = locale === "zh" ? "暂无特别说明。" : "No extra notes yet.";
  const normalizedQuery = query.trim().toLowerCase();
  const visibleSections = useMemo(
    () => filterManualSections(content.sections, normalizedQuery),
    [content.sections, normalizedQuery],
  );
  const visibleItemCount = visibleSections.reduce((total, section) => total + (section.items?.length ?? 0), 0);
  const totalItemCount = content.sections.reduce((total, section) => total + (section.items?.length ?? 0), 0);
  const visibleHeadings = visibleSections.map((section) => section.heading);
  const allVisibleExpanded = visibleHeadings.length > 0 && visibleHeadings.every((heading) => expandedHeadings.has(heading));

  function toggleSection(heading: string) {
    setExpandedHeadings((current) => {
      const next = new Set(current);
      if (next.has(heading)) {
        next.delete(heading);
      } else {
        next.add(heading);
      }
      return next;
    });
  }

  function toggleAllVisibleSections() {
    setExpandedHeadings((current) => {
      const next = new Set(current);
      if (allVisibleExpanded) {
        visibleHeadings.forEach((heading) => next.delete(heading));
      } else {
        visibleHeadings.forEach((heading) => next.add(heading));
      }
      return next;
    });
  }

  return (
    <article className="flex flex-1 flex-col gap-[var(--app-section-gap)] px-[var(--app-page-x)] py-[var(--app-page-y)]">
      <PageHeader
        actions={
          <button
            className="inline-flex h-10 items-center justify-center gap-2 whitespace-nowrap rounded-xl border border-theme-control-border bg-theme-control/95 px-3 text-body-sm font-semibold text-theme-control-fg shadow-[inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.42)] transition-colors hover:bg-theme-control-hover hover:text-on-surface"
            onClick={onBack}
            type="button"
          >
            <ArrowLeft size={16} />
            <span>{backLabel}</span>
          </button>
        }
        actionsClassName="flex justify-end"
        description={content.subtitle}
        eyebrow={eyebrow}
        icon={<BookOpen size={21} />}
        title={content.title}
      />

      <section className="grid gap-4 rounded-xl border border-theme-card-border bg-theme-card/82 p-5 shadow-[0_14px_34px_rgb(var(--theme-panel-shadow)/0.10)]">
        <div className="flex flex-wrap items-center gap-2">
          <span className="inline-flex items-center gap-2 rounded-md border border-theme-control-border bg-theme-control px-2 py-1 font-mono text-code-md text-on-surface-variant">
            <Route size={14} />
            {routeLabel}: {routeKey}
          </span>
          <span className="rounded-md border border-theme-control-border bg-theme-control px-2 py-1 text-label-caps text-on-surface-variant">
            {visibleSections.length}/{content.sections.length} {sectionLabel}
          </span>
          <span className="rounded-md border border-theme-control-border bg-theme-control px-2 py-1 text-label-caps text-on-surface-variant">
            {visibleItemCount}/{totalItemCount} {stepLabel}
          </span>
        </div>
        <p className="max-w-4xl text-body-md leading-7 text-on-surface">{content.overview}</p>

        <label className="relative block max-w-2xl">
          <span className="sr-only">{searchLabel}</span>
          <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-on-surface-muted" />
          <input
            className="h-11 w-full rounded-xl border border-theme-control-border bg-theme-control/85 pl-10 pr-3 text-body-sm text-on-surface outline-none shadow-[inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.35)] transition-colors placeholder:text-on-surface-muted focus:border-primary/70"
            onChange={(event) => setQuery(event.target.value)}
            placeholder={searchPlaceholder}
            type="search"
            value={query}
          />
        </label>
        <div>
          <button
            className="inline-flex h-9 items-center justify-center gap-2 rounded-lg border border-theme-control-border bg-theme-control px-3 text-body-sm font-semibold text-theme-control-fg transition-colors hover:bg-theme-control-hover hover:text-on-surface"
            onClick={toggleAllVisibleSections}
            type="button"
          >
            {allVisibleExpanded ? collapseAllLabel : expandAllLabel}
          </button>
        </div>
      </section>

      <div className="grid gap-4 lg:grid-cols-[280px_minmax(0,1fr)]">
        <aside className="rounded-xl border border-theme-card-border bg-theme-card/72 p-4 lg:sticky lg:top-20 lg:self-start">
          <div className="flex items-center gap-2 text-primary">
            <ListChecks size={17} />
            <h2 className="text-title-sm text-on-surface">{overviewLabel}</h2>
          </div>
          <nav className="mt-4 grid gap-2" aria-label={sectionLabel}>
            {content.sections.map((section, index) => {
              const visible = visibleSections.some((candidate) => candidate.heading === section.heading);

              return (
                <a
                  className={`rounded-lg border px-3 py-2 text-body-sm transition-colors ${
                    visible
                      ? "border-theme-control-border bg-theme-control/70 text-on-surface hover:border-primary/60"
                      : "border-transparent text-on-surface-muted opacity-55"
                  }`}
                  href={`#manual-section-${index}`}
                  key={section.heading}
                >
                  {section.heading}
                </a>
              );
            })}
          </nav>
        </aside>

        <div className="grid gap-4">
          {visibleSections.length === 0 ? (
            <section className="rounded-xl border border-theme-card-border bg-theme-card/78 p-8 text-center text-body-sm text-on-surface-variant">
              {noResultsLabel}
            </section>
          ) : (
            visibleSections.map((section) => {
              const sectionIndex = content.sections.findIndex((candidate) => candidate.heading === section.heading);
              const expanded = expandedHeadings.has(section.heading);
              const steps = section.steps ?? section.items ?? [];
              const outcomes = section.outcomes ?? (section.body ? [section.body] : section.items?.slice(0, 2) ?? []);
              const cautions = section.cautions ?? [];
              const keywords = section.keywords ?? [];

              return (
                <section className="overflow-hidden rounded-xl border border-theme-card-border bg-theme-card/78" id={`manual-section-${sectionIndex}`} key={section.heading}>
                  <button
                    aria-expanded={expanded}
                    className="grid w-full grid-cols-[minmax(0,1fr)_auto] items-start gap-4 p-5 text-left transition-colors hover:bg-theme-control/34"
                    onClick={() => toggleSection(section.heading)}
                    type="button"
                  >
                    <span className="flex min-w-0 items-start gap-3">
                      <span className="mt-0.5 grid size-8 shrink-0 place-items-center rounded-lg border border-theme-control-border bg-theme-control text-primary">
                        <CheckCircle2 size={17} />
                      </span>
                      <span className="min-w-0">
                        <span className="block text-title-sm text-on-surface">{section.heading}</span>
                        {section.body ? <span className="mt-2 block text-body-sm leading-6 text-on-surface-variant">{section.body}</span> : null}
                      </span>
                    </span>
                    <ChevronDown
                      className={`mt-1 size-5 text-on-surface-muted transition-transform ${expanded ? "rotate-180" : ""}`}
                    />
                  </button>
                  {expanded ? (
                    <div className="border-t border-theme-card-border/70 p-5 pt-4">
                      <div className="grid gap-3 xl:grid-cols-3">
                        <ManualInfoBlock emptyLabel={emptyBlockLabel} icon={<Lightbulb size={16} />} items={outcomes} title={outcomesLabel} />
                        <ManualInfoBlock emptyLabel={emptyBlockLabel} icon={<Target size={16} />} items={steps} ordered title={stepsBlockLabel} />
                        <ManualInfoBlock caution emptyLabel={emptyBlockLabel} icon={<AlertTriangle size={16} />} items={cautions} title={cautionsLabel} />
                      </div>

                      {keywords.length ? (
                        <div className="mt-4 flex flex-wrap items-center gap-2">
                          <span className="text-label-caps text-on-surface-muted">{keywordsLabel}</span>
                          {keywords.map((keyword) => (
                            <span
                              className="rounded-md border border-theme-control-border bg-theme-control/65 px-2 py-1 text-code-sm text-on-surface-variant"
                              key={`${section.heading}-${keyword}`}
                            >
                              {keyword}
                            </span>
                          ))}
                        </div>
                      ) : null}
                    </div>
                  ) : null}
                </section>
              );
            })
          )}
        </div>
      </div>
    </article>
  );
}

function ManualInfoBlock({
  caution = false,
  icon,
  items,
  emptyLabel,
  ordered = false,
  title,
}: {
  caution?: boolean;
  emptyLabel: string;
  icon: ReactNode;
  items: string[];
  ordered?: boolean;
  title: string;
}) {
  const ListTag = ordered ? "ol" : "ul";

  return (
    <section className="rounded-lg border border-theme-control-border/70 bg-theme-control/35 p-4">
      <div className={`flex items-center gap-2 ${caution ? "text-status-conflict" : "text-primary"}`}>
        {icon}
        <h3 className="text-label-caps text-on-surface">{title}</h3>
      </div>
      {items.length ? (
        <ListTag className="mt-3 grid gap-2 text-body-sm leading-6 text-on-surface-variant">
          {items.map((item, index) => (
            <li className="grid grid-cols-[auto_minmax(0,1fr)] gap-2" key={item}>
              <span
                className={`mt-[0.3rem] grid shrink-0 place-items-center ${
                  ordered ? "size-5 rounded-md bg-primary/14 text-code-sm font-semibold text-primary" : "mt-[0.45rem] size-1.5 rounded-full bg-current text-primary"
                }`}
                aria-hidden="true"
              >
                {ordered ? index + 1 : null}
              </span>
              <span>{item}</span>
            </li>
          ))}
        </ListTag>
      ) : (
        <p className="mt-3 text-body-sm text-on-surface-muted">{emptyLabel}</p>
      )}
    </section>
  );
}

function filterManualSections(sections: ManualSection[], normalizedQuery: string) {
  if (!normalizedQuery) {
    return sections;
  }

  return sections
    .map((section) => {
      const sectionPayload = [
        section.heading,
        section.body ?? "",
        ...(section.items ?? []),
        ...(section.outcomes ?? []),
        ...(section.steps ?? []),
        ...(section.cautions ?? []),
        ...(section.keywords ?? []),
      ]
        .join(" ")
        .toLowerCase();
      const sectionMatches = sectionPayload.includes(normalizedQuery);
      const matchingItems = section.items?.filter((item) => item.toLowerCase().includes(normalizedQuery)) ?? [];

      if (sectionMatches) {
        return section;
      }

      if (matchingItems.length > 0) {
        return { ...section, items: matchingItems };
      }

      return null;
    })
    .filter((section): section is ManualSection => Boolean(section));
}
