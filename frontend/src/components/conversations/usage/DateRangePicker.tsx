import React, { useState, useEffect, useRef } from "react";
import { Calendar as CalendarIcon, ChevronLeft, ChevronRight, X } from "lucide-react";
import { cn } from "../../../lib/utils";

export interface DateRangePickerProps {
  startDate?: string | null;
  endDate?: string | null;
  onChange: (startDate: string | null, endDate: string | null) => void;
  className?: string;
  placeholder?: string;
}

function toDateString(d: Date): string {
  const year = d.getFullYear();
  const month = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

function parseDateString(str?: string | null): Date | null {
  if (!str) return null;
  const parts = str.split("-").map(Number);
  if (parts.length !== 3 || isNaN(parts[0]) || isNaN(parts[1]) || isNaN(parts[2])) {
    return null;
  }
  return new Date(parts[0], parts[1] - 1, parts[2]);
}

interface CalendarDay {
  date: Date;
  dateStr: string;
  isCurrentMonth: boolean;
  isToday: boolean;
}

export const DateRangePicker: React.FC<DateRangePickerProps> = ({
  startDate,
  endDate,
  onChange,
  className,
  placeholder = "选择日期范围",
}) => {
  const [isOpen, setIsOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  const initialDate = parseDateString(endDate) || parseDateString(startDate) || new Date();
  const [viewYear, setViewYear] = useState<number>(initialDate.getFullYear());
  const [viewMonth, setViewMonth] = useState<number>(initialDate.getMonth());

  // Pending selection state when user clicks the first date
  const [pendingStart, setPendingStart] = useState<string | null>(null);
  const [hoverDate, setHoverDate] = useState<string | null>(null);

  // Sync view month when popover opens or startDate changes
  useEffect(() => {
    if (isOpen) {
      const active = parseDateString(startDate) || new Date();
      setViewYear(active.getFullYear());
      setViewMonth(active.getMonth());
      setPendingStart(null);
      setHoverDate(null);
    }
  }, [isOpen, startDate]);

  // Handle outside click and Escape key
  useEffect(() => {
    if (!isOpen) return;

    const handleClickOutside = (e: MouseEvent) => {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
        setIsOpen(false);
      }
    };

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setIsOpen(false);
      }
    };

    document.addEventListener("mousedown", handleClickOutside);
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("mousedown", handleClickOutside);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [isOpen]);

  const handlePrevMonth = () => {
    if (viewMonth === 0) {
      setViewYear((y) => y - 1);
      setViewMonth(11);
    } else {
      setViewMonth((m) => m - 1);
    }
  };

  const handleNextMonth = () => {
    if (viewMonth === 11) {
      setViewYear((y) => y + 1);
      setViewMonth(0);
    } else {
      setViewMonth((m) => m + 1);
    }
  };

  // Generate 42 calendar cells (Monday-first)
  const days: CalendarDay[] = React.useMemo(() => {
    const result: CalendarDay[] = [];
    const firstDayOfMonth = new Date(viewYear, viewMonth, 1);
    const lastDayOfMonth = new Date(viewYear, viewMonth + 1, 0);
    const todayStr = toDateString(new Date());

    // Monday is 1, Sunday is 0 -> Mon=0, ..., Sun=6
    let startDayOfWeek = firstDayOfMonth.getDay() - 1;
    if (startDayOfWeek === -1) startDayOfWeek = 6;

    // Previous month tail
    for (let i = startDayOfWeek - 1; i >= 0; i--) {
      const d = new Date(viewYear, viewMonth, -i);
      const dateStr = toDateString(d);
      result.push({
        date: d,
        dateStr,
        isCurrentMonth: false,
        isToday: dateStr === todayStr,
      });
    }

    // Current month days
    for (let i = 1; i <= lastDayOfMonth.getDate(); i++) {
      const d = new Date(viewYear, viewMonth, i);
      const dateStr = toDateString(d);
      result.push({
        date: d,
        dateStr,
        isCurrentMonth: true,
        isToday: dateStr === todayStr,
      });
    }

    // Next month head to pad to 42 cells
    const remaining = 42 - result.length;
    for (let i = 1; i <= remaining; i++) {
      const d = new Date(viewYear, viewMonth + 1, i);
      const dateStr = toDateString(d);
      result.push({
        date: d,
        dateStr,
        isCurrentMonth: false,
        isToday: dateStr === todayStr,
      });
    }

    return result;
  }, [viewYear, viewMonth]);

  const handleDayClick = (dayStr: string) => {
    if (!pendingStart) {
      // First click: set pending start
      setPendingStart(dayStr);
    } else {
      // Second click: set range and commit
      let start = pendingStart;
      let end = dayStr;
      if (start > end) {
        const tmp = start;
        start = end;
        end = tmp;
      }
      onChange(start, end);
      setPendingStart(null);
      setIsOpen(false);
    }
  };

  const handleClear = (e?: React.MouseEvent) => {
    e?.stopPropagation();
    onChange(null, null);
    setPendingStart(null);
    setIsOpen(false);
  };

  const handleApplyPreset = (daysAgo: number) => {
    const today = new Date();
    const end = toDateString(today);
    const start = new Date(today);
    start.setDate(today.getDate() - (daysAgo - 1));
    onChange(toDateString(start), end);
    setIsOpen(false);
  };

  const handleApplyThisMonth = () => {
    const today = new Date();
    const start = new Date(today.getFullYear(), today.getMonth(), 1);
    onChange(toDateString(start), toDateString(today));
    setIsOpen(false);
  };

  // Determine date highlighting
  const effectiveStart = pendingStart || startDate;
  const effectiveEnd = pendingStart ? hoverDate || pendingStart : endDate;

  const normalizedRange = React.useMemo(() => {
    if (!effectiveStart || !effectiveEnd) return null;
    return effectiveStart <= effectiveEnd
      ? { start: effectiveStart, end: effectiveEnd }
      : { start: effectiveEnd, end: effectiveStart };
  }, [effectiveStart, effectiveEnd]);

  const isRangeActive = Boolean(startDate && endDate);

  const displayLabel = isRangeActive
    ? `${startDate} ~ ${endDate}`
    : startDate
      ? `${startDate} 起`
      : placeholder;

  const weekHeaders = ["一", "二", "三", "四", "五", "六", "日"];

  return (
    <div ref={containerRef} className={cn("relative inline-block", className)}>
      {/* Trigger Button */}
      <button
        type="button"
        onClick={() => setIsOpen(!isOpen)}
        className={cn(
          "flex h-9 items-center gap-2 rounded-xl border border-theme-control-border/80 bg-theme-control/60 px-3 text-body-sm font-medium text-theme-control-fg shadow-[var(--theme-shadow-control-inset)] cursor-pointer transition-all",
          "hover:border-theme-nav-active-border/60 hover:text-on-surface",
          isOpen && "border-theme-nav-active-border ring-1 ring-primary/20",
          isRangeActive && "text-primary font-medium border-primary/40 bg-primary/5",
        )}
      >
        <CalendarIcon className="h-4 w-4 shrink-0 text-on-surface-variant" />
        <span className="whitespace-nowrap">{displayLabel}</span>
        {isRangeActive && (
          <span
            role="button"
            tabIndex={0}
            onClick={handleClear}
            onKeyDown={(e) => {
              if (e.key === "Enter" || e.key === " ") {
                e.preventDefault();
                handleClear();
              }
            }}
            className="ml-1 rounded-full p-0.5 text-on-surface-variant/70 hover:bg-theme-control-active hover:text-on-surface cursor-pointer"
            title="清空范围"
          >
            <X className="h-3 w-3" />
          </span>
        )}
      </button>

      {/* Popover Calendar Modal */}
      {isOpen && (
        <div className="absolute left-0 top-full z-50 mt-2 w-[304px] rounded-2xl border border-theme-card-border/85 bg-theme-card-header/95 p-3.5 shadow-[var(--theme-shadow-modal),inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.2)] backdrop-blur-xl animate-in fade-in-0 zoom-in-95">
          {/* Header Month / Year Navigation */}
          <div className="mb-3 flex items-center justify-between">
            <button
              type="button"
              onClick={handlePrevMonth}
              className="flex h-7 w-7 items-center justify-center rounded-xl border border-theme-control-border/60 bg-theme-control/40 text-on-surface-variant transition-colors hover:bg-theme-control hover:text-on-surface cursor-pointer"
              title="上一月"
            >
              <ChevronLeft className="h-4 w-4" />
            </button>
            <span className="text-body-sm font-semibold text-on-surface">
              {viewYear}年 {viewMonth + 1}月
            </span>
            <button
              type="button"
              onClick={handleNextMonth}
              className="flex h-7 w-7 items-center justify-center rounded-xl border border-theme-control-border/60 bg-theme-control/40 text-on-surface-variant transition-colors hover:bg-theme-control hover:text-on-surface cursor-pointer"
              title="下一月"
            >
              <ChevronRight className="h-4 w-4" />
            </button>
          </div>

          {/* Weekday Labels */}
          <div className="grid grid-cols-7 mb-1 text-center">
            {weekHeaders.map((w) => (
              <div
                key={w}
                className="text-caption font-medium text-on-surface-variant/60 py-1"
              >
                {w}
              </div>
            ))}
          </div>

          {/* Days Grid */}
          <div className="grid grid-cols-7 gap-y-1">
            {days.map((day) => {
              const isStart = normalizedRange?.start === day.dateStr;
              const isEnd = normalizedRange?.end === day.dateStr;
              const isInside =
                normalizedRange &&
                day.dateStr > normalizedRange.start &&
                day.dateStr < normalizedRange.end;

              return (
                <div
                  key={day.dateStr}
                  className={cn(
                    "relative flex h-8 items-center justify-center",
                    isInside && "bg-primary/10",
                    isStart && normalizedRange?.start !== normalizedRange?.end && "rounded-l-xl bg-primary/10",
                    isEnd && normalizedRange?.start !== normalizedRange?.end && "rounded-r-xl bg-primary/10",
                  )}
                >
                  <button
                    type="button"
                    onClick={() => handleDayClick(day.dateStr)}
                    onMouseEnter={() => {
                      if (pendingStart) {
                        setHoverDate(day.dateStr);
                      }
                    }}
                    className={cn(
                      "flex h-7 w-7 items-center justify-center rounded-xl text-body-sm transition-all cursor-pointer",
                      day.isCurrentMonth
                        ? "text-on-surface"
                        : "text-on-surface-variant/35",
                      day.isToday && !isStart && !isEnd && "border border-primary/40 font-semibold",
                      (isStart || isEnd) &&
                        "bg-primary text-primary-foreground font-semibold shadow-sm hover:bg-primary/90",
                      !isStart && !isEnd && "hover:bg-theme-control hover:text-on-surface",
                    )}
                  >
                    {day.date.getDate()}
                  </button>
                </div>
              );
            })}
          </div>

          {/* Quick Presets & Clear Action */}
          <div className="mt-3.5 flex flex-wrap items-center justify-between gap-1.5 border-t border-theme-card-border/60 pt-3">
            <div className="flex flex-wrap items-center gap-1">
              <button
                type="button"
                onClick={() => handleApplyPreset(1)}
                className="rounded-full border border-theme-control-border/60 bg-theme-control/40 px-2 py-0.5 text-caption text-on-surface-variant transition-colors hover:bg-theme-control hover:text-on-surface cursor-pointer"
              >
                今天
              </button>
              <button
                type="button"
                onClick={() => handleApplyPreset(7)}
                className="rounded-full border border-theme-control-border/60 bg-theme-control/40 px-2 py-0.5 text-caption text-on-surface-variant transition-colors hover:bg-theme-control hover:text-on-surface cursor-pointer"
              >
                近7天
              </button>
              <button
                type="button"
                onClick={() => handleApplyPreset(30)}
                className="rounded-full border border-theme-control-border/60 bg-theme-control/40 px-2 py-0.5 text-caption text-on-surface-variant transition-colors hover:bg-theme-control hover:text-on-surface cursor-pointer"
              >
                近30天
              </button>
              <button
                type="button"
                onClick={handleApplyThisMonth}
                className="rounded-full border border-theme-control-border/60 bg-theme-control/40 px-2 py-0.5 text-caption text-on-surface-variant transition-colors hover:bg-theme-control hover:text-on-surface cursor-pointer"
              >
                本月
              </button>
            </div>
            {isRangeActive && (
              <button
                type="button"
                onClick={handleClear}
                className="text-caption text-on-surface-variant/80 hover:text-primary transition-colors cursor-pointer"
              >
                清空
              </button>
            )}
          </div>
        </div>
      )}
    </div>
  );
};
