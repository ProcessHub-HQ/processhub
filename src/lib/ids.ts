let counter = 0;

/** Stable-enough client-side ids for draft rows; no secure randomness needed. */
export function newDraftId(prefix: string): string {
  counter += 1;
  return `${prefix}-${Date.now().toString(36)}-${counter}`;
}
