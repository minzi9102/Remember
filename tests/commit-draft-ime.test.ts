import { describe, expect, it } from "vitest";

import {
  isImeBoundaryKey,
  shouldRollbackSeededCommitDraft,
} from "../src/application/commit-draft-ime";

describe("commit draft ime guards", () => {
  it("detects ime boundary keyboard events", () => {
    expect(isImeBoundaryKey({ key: "Process", keyCode: 0 })).toBe(true);
    expect(isImeBoundaryKey({ key: "Unidentified", keyCode: 0 })).toBe(true);
    expect(isImeBoundaryKey({ key: "a", keyCode: 229 })).toBe(true);
    expect(isImeBoundaryKey({ key: "a", keyCode: 0 })).toBe(false);
  });

  it("rolls back seeded draft only when it still matches the seed", () => {
    expect(shouldRollbackSeededCommitDraft("n", "n")).toBe(true);
    expect(shouldRollbackSeededCommitDraft("", "n")).toBe(false);
    expect(shouldRollbackSeededCommitDraft("ni", "n")).toBe(false);
    expect(shouldRollbackSeededCommitDraft("n", null)).toBe(false);
  });
});
