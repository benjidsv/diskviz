/**
 * Tests for src/components/charts/vizColor.ts
 *
 * Covers: sizeColorRgb, neutralRgb, activenessColorRgb.
 * Time-sensitive tests use vi.useFakeTimers().
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { sizeColorRgb, neutralRgb, activenessColorRgb } from "./vizColor";

const TWO_STOPS = ["#000000", "#ffffff"];

// ── sizeColorRgb ────────────────────────────────────────────────────────────

describe("sizeColorRgb", () => {
  it("returns the last stop (t=1) when min === max (degenerate range)", () => {
    // logMax === logMin → t set to 1
    const rgb = sizeColorRgb(500, 500, 500, TWO_STOPS);
    expect(rgb).toEqual({ r: 255, g: 255, b: 255 });
  });

  it("returns near the first stop when size equals minSize", () => {
    const rgb = sizeColorRgb(1, 1, 1000, TWO_STOPS);
    // log(1+1)=0.693, log(1+1)=0.693 → t = 0 → first stop
    expect(rgb.r).toBeCloseTo(0, 1);
  });

  it("returns near the last stop when size equals maxSize", () => {
    const rgb = sizeColorRgb(1000, 1, 1000, TWO_STOPS);
    // t approaches 1 → last stop
    expect(rgb.r).toBeGreaterThan(240);
  });

  it("clamps t to [0,1] — size below minSize stays at first stop", () => {
    const rgb = sizeColorRgb(0, 100, 1000, TWO_STOPS);
    // log(0+1)=0 < log(100+1) → t < 0 → clamped to 0 → first stop
    expect(rgb).toEqual({ r: 0, g: 0, b: 0 });
  });
});

// ── neutralRgb ──────────────────────────────────────────────────────────────

describe("neutralRgb", () => {
  it("returns the first stop colour", () => {
    const rgb = neutralRgb(["#ff0000", "#00ff00"]);
    expect(rgb).toEqual({ r: 255, g: 0, b: 0 });
  });

  it("falls back to #808080 when stops is empty", () => {
    const rgb = neutralRgb([]);
    expect(rgb).toEqual({ r: 128, g: 128, b: 128 });
  });
});

// ── activenessColorRgb ──────────────────────────────────────────────────────

describe("activenessColorRgb", () => {
  const NOW_S = 1_735_689_600;
  const NOW_MS = NOW_S * 1000;

  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(NOW_MS);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("returns null when medianMtime is falsy (undefined)", () => {
    expect(activenessColorRgb(undefined, TWO_STOPS, 365)).toBeNull();
  });

  it("returns null when medianMtime is 0", () => {
    expect(activenessColorRgb(0, TWO_STOPS, 365)).toBeNull();
  });

  it("returns null when thresholdDays <= 0", () => {
    expect(activenessColorRgb(NOW_S - 1000, TWO_STOPS, 0)).toBeNull();
    expect(activenessColorRgb(NOW_S - 1000, TWO_STOPS, -1)).toBeNull();
  });

  it("returns a non-null RGB when mtime and threshold are valid", () => {
    // 30 days old, threshold 365 → t ≈ 0.082
    const rgb = activenessColorRgb(NOW_S - 30 * 86400, TWO_STOPS, 365);
    expect(rgb).not.toBeNull();
    expect(rgb!.r).toBeGreaterThanOrEqual(0);
    expect(rgb!.r).toBeLessThanOrEqual(255);
  });

  it("returns near the first stop for a very recent file (t ≈ 0)", () => {
    // 1 second old, threshold 365 days → t ≈ 0
    const rgb = activenessColorRgb(NOW_S - 1, TWO_STOPS, 365);
    expect(rgb!.r).toBeLessThan(5);
  });

  it("returns near the last stop for a very old file (t ≈ 1)", () => {
    // 1000 days old, threshold 365 days → t = 1 (clamped)
    const rgb = activenessColorRgb(NOW_S - 1000 * 86400, TWO_STOPS, 365);
    expect(rgb!.r).toBeGreaterThan(250);
  });
});
