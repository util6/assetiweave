export function isHexColor(value: string) {
  return /^#[0-9a-fA-F]{6}$/.test(value);
}
