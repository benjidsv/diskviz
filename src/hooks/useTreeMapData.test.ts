/**
 * Tests for src/hooks/useTreeMapData.ts
 *
 * The hook itself is a useMemo wrapper and requires renderHook from
 * @testing-library/react. For now we test the pure, exported helpers:
 * - OTHER_NODE_ID sentinel value
 * - isOtherNode guard
 */

import { describe, it, expect } from "vitest";
import { OTHER_NODE_ID, isOtherNode } from "./useTreeMapData";
import type { FileNode } from "@/types";

// ── OTHER_NODE_ID ────────────────────────────────────────────────────────────

describe("OTHER_NODE_ID", () => {
  it("is the string '__other__'", () => {
    expect(OTHER_NODE_ID).toBe("__other__");
  });
});

// ── isOtherNode ──────────────────────────────────────────────────────────────

/** Minimal FileNode factory for tests. */
function makeNode(id: string): FileNode {
  return {
    id,
    name: id,
    path: `/tmp/${id}`,
    type: "directory",
    size: 0,
    fileCount: 0,
    dirCount: 0,
  };
}

describe("isOtherNode", () => {
  it("returns false for null", () => {
    expect(isOtherNode(null)).toBe(false);
  });

  it("returns false for undefined", () => {
    expect(isOtherNode(undefined)).toBe(false);
  });

  it("returns true for a node whose id === OTHER_NODE_ID", () => {
    const node = makeNode(OTHER_NODE_ID);
    expect(isOtherNode(node)).toBe(true);
  });

  it("returns false for a node whose id !== OTHER_NODE_ID", () => {
    expect(isOtherNode(makeNode("0"))).toBe(false);
    expect(isOtherNode(makeNode("42"))).toBe(false);
    expect(isOtherNode(makeNode("__OTHER__"))).toBe(false); // case-sensitive
  });
});
