import { vi } from 'vitest';

export type MockObject<T extends readonly string[]> = {
  [K in T[number]]: ReturnType<typeof vi.fn>;
};

export function createMockObject<const T extends readonly string[]>(
  methods: T,
): MockObject<T> {
  return Object.fromEntries(
    methods.map((method) => [method, vi.fn()]),
  ) as MockObject<T>;
}
