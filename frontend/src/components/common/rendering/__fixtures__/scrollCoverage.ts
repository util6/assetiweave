/** DEV/test-only geometry probe. It does not assert that the compositor painted pixels. */
export const SCROLL_JUMP_RATIOS = [
  0, 0.75, 0.15, 0.9, 0.3, 1, 0, 0.85, 0.1, 0.65, 0.25, 1, 0.05, 0.8, 0.2, 0.6,
] as const;

export function intervalCoverage(
  start: number,
  end: number,
  intervals: readonly (readonly [number, number])[],
): number {
  if (!Number.isFinite(start) || !Number.isFinite(end) || end <= start)
    return 0;
  let cursor = start;
  let covered = 0;
  for (const [top, bottom] of [...intervals].sort((a, b) => a[0] - b[0])) {
    const left = Math.max(start, cursor, top);
    const right = Math.min(end, bottom);
    if (right > left) covered += right - left;
    cursor = Math.max(cursor, right);
  }
  return covered / (end - start);
}

export function readScrollCoverage(surface: HTMLElement) {
  const collection = surface.querySelector<HTMLElement>(
    "[data-virtualized-collection], .asset-list-surface",
  );
  const bounds = surface.getBoundingClientRect();
  const content = collection?.getBoundingClientRect() ?? bounds;
  const collectionStyle = collection ? getComputedStyle(collection) : null;
  const pixels = (value?: string) => Number.parseFloat(value ?? "0") || 0;
  const insetTop =
    pixels(collectionStyle?.paddingTop) +
    pixels(collectionStyle?.borderTopWidth);
  const insetBottom =
    pixels(collectionStyle?.paddingBottom) +
    pixels(collectionStyle?.borderBottomWidth);
  const gap = pixels(collectionStyle?.rowGap);
  // Exclude intentional padding outside the collection, but not gaps inside it.
  const start = Math.max(
    bounds.top + surface.clientTop,
    content.top + insetTop,
  );
  const end = Math.min(
    bounds.top + surface.clientTop + surface.clientHeight,
    content.bottom - insetBottom,
  );
  const rows = [
    ...surface.querySelectorAll<HTMLElement>(
      "[data-virtual-item-key], [data-asset-id]",
    ),
  ];
  const intervals = rows.map((row) => {
    const rect = row.getBoundingClientRect();
    return [rect.top - gap / 2, rect.bottom + gap / 2] as const;
  });
  const viewportValid =
    surface.clientHeight > 0 &&
    surface.clientHeight <= window.innerHeight &&
    collection !== null &&
    content.height > 0 &&
    end > start;
  return {
    viewportValid,
    clientHeight: surface.clientHeight,
    scrollHeight: surface.scrollHeight,
    scrollTop: surface.scrollTop,
    mountedKeys: rows.map(
      (row) => row.dataset.virtualItemKey ?? row.dataset.assetId,
    ),
    intervals,
    coverage: viewportValid ? intervalCoverage(start, end, intervals) : 0,
    phase: surface.dataset.scrollPhase,
  };
}

export async function sampleScrollCoverage(surface: HTMLElement) {
  const samples = [];
  const initialOffset = surface.scrollTop;
  try {
    for (const ratio of SCROLL_JUMP_RATIOS) {
      surface.scrollTop = ratio * (surface.scrollHeight - surface.clientHeight);
      for (let frame = 1; frame <= 3; frame++) {
        await new Promise<void>((resolve) =>
          requestAnimationFrame(() => resolve()),
        );
        samples.push({ ratio, frame, ...readScrollCoverage(surface) });
      }
    }
  } finally {
    surface.scrollTop = initialOffset;
  }
  return samples;
}
