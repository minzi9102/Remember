interface ImeBoundaryKeyboardEvent {
  key: string;
  keyCode: number;
}

export function isImeBoundaryKey(event: ImeBoundaryKeyboardEvent): boolean {
  return event.key === "Process" || event.key === "Unidentified" || event.keyCode === 229;
}

export function shouldRollbackSeededCommitDraft(
  commitDraft: string,
  seededChar: string | null,
): boolean {
  return seededChar !== null && commitDraft === seededChar;
}
