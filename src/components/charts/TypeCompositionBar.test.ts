/**
 * Tests for src/components/charts/TypeCompositionBar.tsx
 *
 * Covers the pure logic helpers: compositionSlices and topTypesText.
 * The React component itself is not rendered here (no @testing-library needed).
 */

import { describe, it, expect } from "vitest";
import { compositionSlices, topTypesText } from "./TypeCompositionBar";
import type { CompSlice } from "./TypeCompositionBar";

const COLORS = ["#ff0000", "#00ff00", "#0000ff", "#ffff00"] as const;

// ── compositionSlices ───────────────────────────────────────────────────────

describe("compositionSlices", () => {
  it("returns [] when total <= 0 (no types, no other)", () => {
    expect(compositionSlices([], 0, COLORS)).toEqual([]);
    expect(compositionSlices(undefined, 0, COLORS)).toEqual([]);
  });

  it("returns [] when types are empty and other is also 0", () => {
    expect(compositionSlices([], undefined, COLORS)).toEqual([]);
  });

  it("converts empty extension to 'no ext'", () => {
    const slices = compositionSlices([{ ext: "", size: 100 }], 0, COLORS);
    expect(slices[0].label).toBe("no ext");
  });

  it("uppercases non-empty extensions", () => {
    const slices = compositionSlices(
      [{ ext: "png", size: 100 }, { ext: "mp4", size: 200 }],
      0,
      COLORS,
    );
    expect(slices[0].label).toBe("PNG");
    expect(slices[1].label).toBe("MP4");
  });

  it("appends 'Other' slice only when otherSize > 0", () => {
    const noOther = compositionSlices([{ ext: "rs", size: 100 }], 0, COLORS);
    expect(noOther.find((s) => s.label === "Other")).toBeUndefined();

    const withOther = compositionSlices([{ ext: "rs", size: 100 }], 50, COLORS);
    const other = withOther.find((s) => s.label === "Other");
    expect(other).toBeDefined();
    expect(other!.size).toBe(50);
  });

  it("computes pct as fraction of total", () => {
    const slices = compositionSlices(
      [{ ext: "png", size: 300 }, { ext: "jpg", size: 700 }],
      0,
      COLORS,
    );
    expect(slices[0].pct).toBeCloseTo(0.3, 5);
    expect(slices[1].pct).toBeCloseTo(0.7, 5);
  });

  it("wraps color using i % colors.length", () => {
    // 5 types but only 4 colors → 5th type wraps to COLORS[0]
    const types = Array.from({ length: 5 }, (_, i) => ({ ext: `ext${i}`, size: 10 }));
    const slices = compositionSlices(types, 0, COLORS);
    expect(slices[4].color).toBe(COLORS[4 % 4]);
  });

  it("includes otherSize in total for pct calculation", () => {
    const slices = compositionSlices([{ ext: "rs", size: 100 }], 100, COLORS);
    // total = 200; rs pct = 0.5, other pct = 0.5
    expect(slices[0].pct).toBeCloseTo(0.5, 5);
    expect(slices[1].pct).toBeCloseTo(0.5, 5);
  });
});

// ── topTypesText ─────────────────────────────────────────────────────────────

describe("topTypesText", () => {
  const slices: CompSlice[] = [
    { label: "PNG",  size: 670, pct: 0.67, color: "#ff0000" },
    { label: "MP4",  size: 180, pct: 0.18, color: "#00ff00" },
    { label: "WEBP", size: 90,  pct: 0.09, color: "#0000ff" },
    { label: "GIF",  size: 60,  pct: 0.06, color: "#ffff00" },
  ];

  it("returns top 3 by default", () => {
    expect(topTypesText(slices)).toBe("PNG 67% · MP4 18% · WEBP 9%");
  });

  it("respects a custom count", () => {
    expect(topTypesText(slices, 2)).toBe("PNG 67% · MP4 18%");
    expect(topTypesText(slices, 4)).toBe("PNG 67% · MP4 18% · WEBP 9% · GIF 6%");
  });

  it("returns all slices when count > slices.length", () => {
    const few: CompSlice[] = [
      { label: "PNG", size: 100, pct: 1, color: "#ff0000" },
    ];
    expect(topTypesText(few, 3)).toBe("PNG 100%");
  });

  it("rounds percentages", () => {
    const s: CompSlice[] = [{ label: "X", size: 1, pct: 1 / 3, color: "#f00" }];
    expect(topTypesText(s, 1)).toBe("X 33%");
  });
});
