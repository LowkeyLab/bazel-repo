// This comment must not count as executable code.
export interface Amount {
  value: number;
}

export function choose(value: number): number {
  if (value > 0) {
    return value + 1;
  }
  return value - 1;
}
