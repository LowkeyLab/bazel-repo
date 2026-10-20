// This file has no import from the test.
export interface Unused {
  label: string;
}

export function neverCalled(value: number): number {
  if (value > 0) {
    return value + 10;
  }
  return value - 10;
}
