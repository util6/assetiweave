export function toPanelLayout(
  weights: readonly number[],
): Record<string, number> {
  const count = weights.length;
  if (count === 0) return {};

  const total = weights.reduce((sum, weight) => sum + weight, 0);
  const layout: Record<string, number> = {};

  if (
    !Number.isFinite(total) ||
    total <= 0 ||
    weights.some((w) => !Number.isFinite(w) || w <= 0)
  ) {
    const equalShare = 100 / count;
    for (let index = 0; index < count; index += 1) {
      layout[`column-${index}`] = equalShare;
    }
    return layout;
  }

  for (let index = 0; index < count; index += 1) {
    layout[`column-${index}`] = (weights[index] / total) * 100;
  }

  return layout;
}

export function fromPanelLayout(
  layout: Record<string, number>,
  count: number,
): number[] | null {
  if (count <= 0) return null;

  const weights: number[] = [];
  for (let index = 0; index < count; index += 1) {
    const value = layout[`column-${index}`];
    if (typeof value !== "number" || !Number.isFinite(value) || value <= 0) {
      return null;
    }
    weights.push(value);
  }

  return weights;
}

export function sanitizeColumnWeights(
  weights: unknown,
  fallbackWeights: readonly number[],
): number[] {
  if (!Array.isArray(weights) || weights.length !== fallbackWeights.length) {
    return [...fallbackWeights];
  }

  if (
    weights.some(
      (weight) =>
        typeof weight !== "number" || !Number.isFinite(weight) || weight <= 0,
    )
  ) {
    return [...fallbackWeights];
  }

  const currentTotal = (weights as number[]).reduce((sum, w) => sum + w, 0);
  const fallbackTotal = fallbackWeights.reduce((sum, w) => sum + w, 0);
  if (currentTotal <= 0 || fallbackTotal <= 0) {
    return [...fallbackWeights];
  }

  const scale = fallbackTotal / currentTotal;
  return (weights as number[]).map((w) => w * scale);
}
