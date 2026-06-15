import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ChevronDown, Copy, FileText, FolderOpen, RefreshCw, Trash2 } from "lucide-react";
import { useI18n } from "../../i18n/I18nProvider";
import { getLogSnapshot, openLogDirectory, writeOperationLog, type LogSnapshot } from "../../services/logService";
import {
  clampLogLineLimit,
  DEFAULT_LOG_LINE_LIMIT,
  filterLogContent,
  MAX_LOG_LINE_LIMIT,
  MIN_LOG_LINE_LIMIT,
  type LogLevelFilter,
} from "../../utils/logViewer";
import { abbreviateHomePath } from "../../utils/path";
import { DialogFrame } from "../foundation/DialogFrame";
import { Button } from "../ui/button";
import "./LogViewerModal.css";

interface LogViewerModalProps {
  open: boolean;
  onClose: () => void;
}

const POLL_INTERVAL_MS = 1000;
const FEEDBACK_DURATION_MS = 1200;

export function LogViewerModal({ open, onClose }: LogViewerModalProps) {
  const { t } = useI18n();
  const logsLabel = t("logViewer.logs");
  const logDirLabel = t("logViewer.logDirectory");
  const levelOptions: Array<{ value: LogLevelFilter; label: string }> = useMemo(
    () => [
      { value: "ALL", label: t("logViewer.levels.all") },
      { value: "INFO", label: t("logViewer.levels.info") },
      { value: "WARN", label: t("logViewer.levels.warn") },
      { value: "ERROR", label: t("logViewer.levels.error") },
    ],
    [t],
  );

  const [lineLimit, setLineLimit] = useState(DEFAULT_LOG_LINE_LIMIT);
  const [lineLimitDraft, setLineLimitDraft] = useState(String(DEFAULT_LOG_LINE_LIMIT));
  const [selectedFileName, setSelectedFileName] = useState("");
  const [levelFilter, setLevelFilter] = useState<LogLevelFilter>("ALL");
  const [snapshot, setSnapshot] = useState<LogSnapshot | null>(null);
  const [rawContent, setRawContent] = useState("");
  const [visibleRawContent, setVisibleRawContent] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [copied, setCopied] = useState(false);
  const [pathCopied, setPathCopied] = useState(false);

  const viewRef = useRef<HTMLDivElement>(null);
  const shouldStickToBottomRef = useRef(true);
  const clearMarkerRef = useRef<string | null>(null);

  const updatedAtText = useMemo(() => {
    if (!snapshot?.modified_at_ms) {
      return "-";
    }

    const date = new Date(snapshot.modified_at_ms);
    if (Number.isNaN(date.getTime())) {
      return "-";
    }

    return date.toLocaleString();
  }, [snapshot?.modified_at_ms]);

  const displayedContent = useMemo(
    () => filterLogContent(visibleRawContent, levelFilter),
    [levelFilter, visibleRawContent],
  );

  const applyLineLimit = useCallback(() => {
    const parsed = Number.parseInt(lineLimitDraft.trim(), 10);
    if (!Number.isFinite(parsed)) {
      setLineLimitDraft(String(lineLimit));
      return;
    }

    const next = clampLogLineLimit(parsed);
    setLineLimit(next);
    setLineLimitDraft(String(next));
  }, [lineLimit, lineLimitDraft]);

  const loadSnapshot = useCallback(
    async (showLoading: boolean) => {
      try {
        if (showLoading) {
          setLoading(true);
        }

        const next = await getLogSnapshot(selectedFileName || undefined, lineLimit);
        setSnapshot(next);
        setError("");
        setRawContent(next.content);

        const marker = clearMarkerRef.current;
        let nextVisible = next.content;
        if (marker !== null) {
          if (next.content === marker) {
            nextVisible = "";
          } else if (next.content.startsWith(marker)) {
            nextVisible = next.content.slice(marker.length).replace(/^\n+/, "");
          }

          if (nextVisible.length > 0) {
            clearMarkerRef.current = null;
          }
        }

        setVisibleRawContent(nextVisible);
      } catch (err) {
        setError(String(err));
      } finally {
        if (showLoading) {
          setLoading(false);
        }
      }
    },
    [lineLimit, selectedFileName],
  );

  useEffect(() => {
    if (!open) {
      return;
    }

    void loadSnapshot(true);
    const timer = window.setInterval(() => {
      void loadSnapshot(false);
    }, POLL_INTERVAL_MS);

    return () => {
      window.clearInterval(timer);
    };
  }, [loadSnapshot, open]);

  useEffect(() => {
    clearMarkerRef.current = null;
  }, [selectedFileName]);

  useEffect(() => {
    if (!open) {
      clearMarkerRef.current = null;
      return;
    }

    const view = viewRef.current;
    if (!view || !shouldStickToBottomRef.current) {
      return;
    }
    view.scrollTop = view.scrollHeight;
  }, [displayedContent, open]);

  if (!open) {
    return null;
  }

  const activeFileName = selectedFileName || snapshot?.log_file_name || "";
  const hasFilteredOutContent =
    levelFilter !== "ALL" &&
    visibleRawContent.trim().length > 0 &&
    displayedContent.trim().length === 0;

  function handleClearOutput() {
    clearMarkerRef.current = rawContent;
    setVisibleRawContent("");
    setError("");
    void writeOperationLog("INFO", "log_viewer.clear", "清空日志窗口输出", {
      file: activeFileName,
      visible_chars: visibleRawContent.length,
    });
  }

  async function handleCopyLogs() {
    try {
      await navigator.clipboard.writeText(displayedContent);
      await writeOperationLog("INFO", "log_viewer.copy_logs", "复制日志内容成功", {
        file: activeFileName,
        level: levelFilter,
        copied_chars: displayedContent.length,
      }).catch(() => undefined);
      setCopied(true);
      window.setTimeout(() => setCopied(false), FEEDBACK_DURATION_MS);
    } catch (err) {
      await writeOperationLog("ERROR", "log_viewer.copy_logs", "复制日志内容失败", {
        file: activeFileName,
        error: String(err),
      }).catch(() => undefined);
      setError(String(err));
    }
  }

  async function handleCopyPath() {
    if (!snapshot?.log_file_path) {
      return;
    }

    try {
      await navigator.clipboard.writeText(snapshot.log_file_path);
      await writeOperationLog("INFO", "log_viewer.copy_path", "复制日志文件路径成功", {
        file: activeFileName,
        path: snapshot.log_file_path,
      }).catch(() => undefined);
      setPathCopied(true);
      window.setTimeout(() => setPathCopied(false), FEEDBACK_DURATION_MS);
    } catch (err) {
      await writeOperationLog("ERROR", "log_viewer.copy_path", "复制日志文件路径失败", {
        file: activeFileName,
        error: String(err),
      }).catch(() => undefined);
      setError(String(err));
    }
  }

  async function handleOpenDir() {
    try {
      await openLogDirectory();
    } catch (err) {
      setError(String(err));
    }
  }

  async function handleManualRefresh() {
    await writeOperationLog("INFO", "log_viewer.refresh", "手动刷新日志窗口", {
      file: activeFileName,
      line_limit: lineLimit,
      level: levelFilter,
    }).catch(() => undefined);
    await loadSnapshot(true);
  }

  return (
    <DialogFrame
      className="max-w-[920px]"
      closeLabel={t("common.close")}
      contentClassName="flex min-h-0 flex-col gap-3"
      footer={
        <>
          <Button onClick={onClose} size="sm" type="button" variant="ghost">
            {t("common.close")}
          </Button>
          <Button onClick={() => void handleManualRefresh()} size="sm" type="button" variant="outline">
            <RefreshCw size={15} />
            {t("common.refresh")}
          </Button>
          <Button onClick={handleClearOutput} size="sm" type="button" variant="outline">
            <Trash2 size={15} />
            {t("logViewer.clear")}
          </Button>
          <Button onClick={() => void handleOpenDir()} size="sm" type="button" variant="outline">
            <FolderOpen size={15} />
            {t("common.open")} {logDirLabel}
          </Button>
          <Button onClick={() => void handleCopyPath()} size="sm" type="button" variant="outline">
            <Copy size={15} />
            {pathCopied ? t("common.success") : `${t("common.copy")} ${t("logViewer.filePath")}`}
          </Button>
          <Button onClick={() => void handleCopyLogs()} size="sm" type="button">
            <Copy size={15} />
            {copied ? t("common.success") : `${t("common.copy")} ${logsLabel}`}
          </Button>
        </>
      }
      footerClassName="flex-wrap"
      icon={<FileText size={18} />}
      iconClassName="border-status-update/25 bg-status-update/15 text-status-update"
      onClose={onClose}
      overlayClassName="z-[12020]"
      size="2xl"
      title={logsLabel}
    >
      <div className="log-viewer-meta">
        <div className="log-viewer-meta-item log-viewer-file-item">
          <FileText size={14} />
          {snapshot?.available_files?.length ? (
            <div className="log-viewer-select-wrap">
              <select
                className="log-viewer-select"
                value={activeFileName}
                onChange={(event) => {
                  setSelectedFileName(event.target.value);
                  setError("");
                }}
                aria-label={t("logViewer.fileLabel")}
              >
                {snapshot.available_files.map((file) => (
                  <option key={file.log_file_name} value={file.log_file_name}>
                    {file.log_file_name}
                  </option>
                ))}
              </select>
              <ChevronDown size={14} />
            </div>
          ) : (
            <span className="log-viewer-path-text">-</span>
          )}
        </div>
        <div className="log-viewer-meta-item">
          <FolderOpen size={14} />
          <span className="log-viewer-path-text">{snapshot?.log_dir_path ? abbreviateHomePath(snapshot.log_dir_path) : "-"}</span>
        </div>
        <div className="log-viewer-meta-item">
          <RefreshCw size={14} />
          <span>{updatedAtText}</span>
        </div>
        <div className="log-viewer-toolbar">
          <div className="log-viewer-filter-wrap">
            <span className="log-viewer-line-limit-label">{t("logViewer.levelLabel")}</span>
            <div className="log-viewer-select-wrap log-viewer-level-select-wrap">
              <select
                className="log-viewer-select"
                value={levelFilter}
                onChange={(event) => setLevelFilter(event.target.value as LogLevelFilter)}
                aria-label={t("logViewer.levelLabel")}
              >
                {levelOptions.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.label}
                  </option>
                ))}
              </select>
              <ChevronDown size={14} />
            </div>
          </div>
          <div className="log-viewer-line-limit-wrap">
            <span className="log-viewer-line-limit-label">{t("logViewer.lineLimit", { count: lineLimit })}</span>
            <input
              className="log-viewer-line-limit-input"
              type="number"
              min={MIN_LOG_LINE_LIMIT}
              max={MAX_LOG_LINE_LIMIT}
              value={lineLimitDraft}
              onChange={(event) => setLineLimitDraft(event.target.value)}
              onBlur={applyLineLimit}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  applyLineLimit();
                }
              }}
            />
          </div>
        </div>
      </div>

      <div
        className="log-viewer-content"
        ref={viewRef}
        onScroll={(event) => {
          const target = event.currentTarget;
          const bottomDistance = target.scrollHeight - target.scrollTop - target.clientHeight;
          shouldStickToBottomRef.current = bottomDistance <= 24;
        }}
      >
        {loading && !displayedContent ? (
          <div className="log-viewer-placeholder">{t("common.loading")}</div>
        ) : displayedContent ? (
          <pre>{displayedContent}</pre>
        ) : (
          <div className="log-viewer-placeholder">
            {hasFilteredOutContent ? t("logViewer.noMatches") : t("common.none")}
          </div>
        )}
      </div>

      {error ? <p className="log-viewer-error">{error}</p> : null}
    </DialogFrame>
  );
}
