export function wrapIndex(idx: number, length: number): number {
  return ((idx % length) + length) % length;
}

export function clampIndex(idx: number, length: number): number {
  return Math.min(Math.max(0, length - 1), Math.max(0, idx));
}
