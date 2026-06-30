/**
 * Tests for src/utils/formatters.ts
 *
 * Covers: formatFileSize, formatDuration, formatPercentage, formatAge,
 * activeness. Time-sensitive functions are exercised with vi.useFakeTimers()
 * so tests are deterministic regardless of when they run.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import {
  formatFileSize,
  formatDuration,
  formatPercentage,
  formatAge,
  activeness,
} from "./formatters";

// ── formatFileSize ──────────────────────────────────────────────────────────

describe("formatFileSize", () => {
  it("returns '0 B' for zero", () => {
    expect(formatFileSize(0)).toBe("0 B");
  });

  it("returns '0 B' for negative values", () => {
    expect(formatFileSize(-100)).toBe("0 B");
  });

  it("formats bytes with 0 decimals (B unit)", () => {
    expect(formatFileSize(1)).toBe("1 B");
    expect(formatFileSize(1023)).toBe("1023 B");
  });

  it("formats 1024 bytes as 1.00 KB", () => {
    expect(formatFileSize(1024)).toBe("1.00 KB");
  });

  it("formats 1 MB boundary", () => {
    expect(formatFileSize(1024 * 1024)).toBe("1.00 MB");
  });

  it("formats 1 GB boundary", () => {
    expect(formatFileSize(1024 ** 3)).toBe("1.00 GB");
  });

  it("formats 1 TB boundary", () => {
    expect(formatFileSize(1024 ** 4)).toBe("1.00 TB");
  });

  it("caps at PB (does not go to EB or beyond)", () => {
    // 1024^5 = 1 PB
    expect(formatFileSize(1024 ** 5)).toBe("1.00 PB");
    // 1024^6 would overflow the units array; stays in PB
    expect(formatFileSize(1024 ** 6)).toBe("1024.00 PB");
  });

  it("uses 2 decimal places for KB and higher", () => {
    expect(formatFileSize(2560)).toBe("2.50 KB");
  });
});

// ── formatDuration ──────────────────────────────────────────────────────────

describe("formatDuration", () => {
  it("shows ms for values < 1000", () => {
    expect(formatDuration(0)).toBe("0 ms");
    expect(formatDuration(500)).toBe("500 ms");
    expect(formatDuration(999)).toBe("999 ms");
  });

  it("switches to seconds at exactly 1000 ms", () => {
    expect(formatDuration(1000)).toBe("1.00 s");
  });

  it("formats seconds with 2 decimal places", () => {
    expect(formatDuration(1500)).toBe("1.50 s");
    expect(formatDuration(60000)).toBe("60.00 s");
  });
});

// ── formatPercentage ────────────────────────────────────────────────────────

describe("formatPercentage", () => {
  it("returns '0.0%' when total is 0", () => {
    expect(formatPercentage(100, 0)).toBe("0.0%");
  });

  it("returns '0.0%' when total is falsy", () => {
    // @ts-expect-error testing JS callers passing undefined
    expect(formatPercentage(100, undefined)).toBe("0.0%");
  });

  it("computes correct percentage", () => {
    expect(formatPercentage(50, 100)).toBe("50.0%");
    expect(formatPercentage(1, 3)).toBe("33.3%");
  });

  it("can exceed 100% (no clamping)", () => {
    expect(formatPercentage(200, 100)).toBe("200.0%");
  });
});

// ── formatAge ───────────────────────────────────────────────────────────────

describe("formatAge", () => {
  // Fix 'now' to 2025-01-01T00:00:00Z = 1735689600
  const NOW_S = 1_735_689_600;
  const NOW_MS = NOW_S * 1000;

  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(NOW_MS);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("returns '—' for falsy timestamp", () => {
    expect(formatAge(0)).toBe("—");
  });

  it("returns 'today' for timestamps within the last 24 h", () => {
    expect(formatAge(NOW_S - 3600)).toBe("today"); // 1 hour ago
    expect(formatAge(NOW_S)).toBe("today");         // now
  });

  it("clamps negative ages to 'today'", () => {
    // timestamp in the future
    expect(formatAge(NOW_S + 86400)).toBe("today");
  });

  it("formats days (1–29)", () => {
    expect(formatAge(NOW_S - 1 * 86400)).toBe("1d");
    expect(formatAge(NOW_S - 7 * 86400)).toBe("7d");
    expect(formatAge(NOW_S - 29 * 86400)).toBe("29d");
  });

  it("formats months (30–364 days)", () => {
    expect(formatAge(NOW_S - 30 * 86400)).toBe("1mo");
    expect(formatAge(NOW_S - 60 * 86400)).toBe("2mo");
  });

  it("formats years without remainder months", () => {
    // Exactly 365 days → 1 year, 0 months
    expect(formatAge(NOW_S - 365 * 86400)).toBe("1y");
  });

  it("formats years with remainder months", () => {
    // ~395 days → 1y 1mo
    expect(formatAge(NOW_S - 395 * 86400)).toBe("1y 1mo");
  });
});

// ── activeness ──────────────────────────────────────────────────────────────

describe("activeness", () => {
  const NOW_S = 1_735_689_600;
  const NOW_MS = NOW_S * 1000;

  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(NOW_MS);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("returns Active when thresholdDays <= 0 (early return)", () => {
    const r = activeness(NOW_S - 10000, 0);
    expect(r.score).toBe(0);
    expect(r.label).toBe("Active");
  });

  it("returns Active when unixSeconds is falsy", () => {
    const r = activeness(0, 365);
    expect(r.score).toBe(0);
    expect(r.label).toBe("Active");
  });

  it("clamps score to 0 for future timestamps", () => {
    const r = activeness(NOW_S + 86400, 365);
    expect(r.score).toBe(0);
  });

  it("clamps score to 1 for very old timestamps", () => {
    const r = activeness(NOW_S - 1000 * 86400, 365);
    expect(r.score).toBe(1);
  });

  it("returns 'Active' for score < 0.1", () => {
    // 5 days old, threshold 365 → score ≈ 0.014
    const r = activeness(NOW_S - 5 * 86400, 365);
    expect(r.label).toBe("Active");
  });

  it("returns 'Recent' for score in [0.1, 0.4)", () => {
    // 73 days old, threshold 365 → score ≈ 0.2
    const r = activeness(NOW_S - 73 * 86400, 365);
    expect(r.label).toBe("Recent");
  });

  it("returns 'Stale' for score in [0.4, 0.8)", () => {
    // 219 days old, threshold 365 → score ≈ 0.6
    const r = activeness(NOW_S - 219 * 86400, 365);
    expect(r.label).toBe("Stale");
  });

  it("returns 'Dormant' for score >= 0.8", () => {
    // 360 days old, threshold 365 → score ≈ 0.986
    const r = activeness(NOW_S - 360 * 86400, 365);
    expect(r.label).toBe("Dormant");
  });
});
