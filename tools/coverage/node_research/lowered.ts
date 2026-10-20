// Async lowering must not add original branches.
export async function lowered(flag: boolean): Promise<number> {
  return flag ? await Promise.resolve(9) : 4;
}
