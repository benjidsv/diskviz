/**
 * Tests for src/lib/colorScale.ts
 *
 * Covers: hexToRgb (including edge cases), relativeLuminance, readableInk,
 * interpolateStops, rgbToCss.
 */

import { describe, it, expect } from "vitest";
import {
  hexToRgb,
  relativeLuminance,
  readableInk,
  interpolateStops,
  rgbToCss,
} from "./colorScale";

// ── hexToRgb ────────────────────────────────────────────────────────────────

describe("hexToRgb", () => {
  it("parses a 6-digit hex with leading #", () => {
    expect(hexToRgb("#ff0000")).toEqual({ r: 255, g: 0, b: 0 });
    expect(hexToRgb("#00ff00")).toEqual({ r: 0, g: 255, b: 0 });
    expect(hexToRgb("#0000ff")).toEqual({ r: 0, g: 0, b: 255 });
    expect(hexToRgb("#ffffff")).toEqual({ r: 255, g: 255, b: 255 });
    expect(hexToRgb("#000000")).toEqual({ r: 0, g: 0, b: 0 });
  });

  it("parses a 6-digit hex without leading #", () => {
    expect(hexToRgb("ff8800")).toEqual({ r: 255, g: 136, b: 0 });
  });

  it("trims whitespace before parsing", () => {
    expect(hexToRgb("  #abcdef  ")).toEqual({ r: 171, g: 205, b: 239 });
  });

  it("returns grey fallback {128,128,128} for invalid hex strings", () => {
    expect(hexToRgb("not-a-color")).toEqual({ r: 128, g: 128, b: 128 });
    expect(hexToRgb("")).toEqual({ r: 128, g: 128, b: 128 });
    expect(hexToRgb("#xyz")).toEqual({ r: 128, g: 128, b: 128 });
  });

  it("mis-parses 3-digit shorthand (#fff) as a 6-digit value (known limitation)", () => {
    // The implementation does NOT expand #fff → #ffffff.
    // parseInt("fff", 16) = 4095 = 0x000FFF
    // So r=0, g=15, b=255 — document the actual (not ideal) behaviour.
    const rgb = hexToRgb("#fff");
    expect(rgb).toEqual({ r: 0, g: 15, b: 255 });
  });
});

// ── rgbToCss ────────────────────────────────────────────────────────────────

describe("rgbToCss", () => {
  it("rounds float channel values", () => {
    expect(rgbToCss({ r: 1.6, g: 2.4, b: 255.9 })).toBe("rgb(2,2,256)");
  });

  it("formats integer channels correctly", () => {
    expect(rgbToCss({ r: 255, g: 0, b: 128 })).toBe("rgb(255,0,128)");
  });
});

// ── relativeLuminance ───────────────────────────────────────────────────────

describe("relativeLuminance", () => {
  it("returns ~1 for white", () => {
    const lum = relativeLuminance({ r: 255, g: 255, b: 255 });
    expect(lum).toBeCloseTo(1, 4);
  });

  it("returns 0 for black", () => {
    const lum = relativeLuminance({ r: 0, g: 0, b: 0 });
    expect(lum).toBe(0);
  });

  it("returns a value between 0 and 1 for mid-tones", () => {
    const lum = relativeLuminance({ r: 128, g: 128, b: 128 });
    expect(lum).toBeGreaterThan(0);
    expect(lum).toBeLessThan(1);
  });
});

// ── readableInk ─────────────────────────────────────────────────────────────

describe("readableInk", () => {
  it("returns light ink (#eff1f5) on dark backgrounds", () => {
    // Pure black → luminance 0 < 0.179
    expect(readableInk({ r: 0, g: 0, b: 0 })).toBe("#eff1f5");
  });

  it("returns dark ink (#11111b) on light backgrounds", () => {
    // Pure white → luminance ~1 > 0.179
    expect(readableInk({ r: 255, g: 255, b: 255 })).toBe("#11111b");
  });

  it("straddles the 0.179 threshold correctly", () => {
    // A colour whose luminance is just below 0.179 → light ink
    // rgb(100,100,100): linearized ≈ 0.1329 → lum ≈ 0.1329  < 0.179 → light
    const darkGrey = readableInk({ r: 100, g: 100, b: 100 });
    expect(darkGrey).toBe("#eff1f5");

    // rgb(150,150,150): linearized ≈ 0.3214 → lum ≈ 0.3214 > 0.179 → dark
    const lightGrey = readableInk({ r: 150, g: 150, b: 150 });
    expect(lightGrey).toBe("#11111b");
  });
});

// ── interpolateStops ────────────────────────────────────────────────────────

describe("interpolateStops", () => {
  it("returns grey fallback for empty stops array", () => {
    expect(interpolateStops([], 0.5)).toEqual({ r: 128, g: 128, b: 128 });
  });

  it("returns the only stop for a single-element array", () => {
    expect(interpolateStops(["#ff0000"], 0.5)).toEqual({ r: 255, g: 0, b: 0 });
  });

  it("returns the first stop when t <= 0", () => {
    const stops = ["#000000", "#ffffff"];
    expect(interpolateStops(stops, 0)).toEqual({ r: 0, g: 0, b: 0 });
    expect(interpolateStops(stops, -1)).toEqual({ r: 0, g: 0, b: 0 });
  });

  it("returns the last stop when t >= 1", () => {
    const stops = ["#000000", "#ffffff"];
    expect(interpolateStops(stops, 1)).toEqual({ r: 255, g: 255, b: 255 });
    expect(interpolateStops(stops, 2)).toEqual({ r: 255, g: 255, b: 255 });
  });

  it("interpolates to exact midpoint at t=0.5 for 2-stop ramp", () => {
    const stops = ["#000000", "#ffffff"];
    const mid = interpolateStops(stops, 0.5);
    // Midpoint between 0 and 255 is 127.5
    expect(mid.r).toBeCloseTo(127.5, 5);
    expect(mid.g).toBeCloseTo(127.5, 5);
    expect(mid.b).toBeCloseTo(127.5, 5);
  });

  it("interpolates within the correct segment for 3-stop ramp", () => {
    // stops: black (#000000) → red (#ff0000) → white (#ffffff)
    const stops = ["#000000", "#ff0000", "#ffffff"];
    // t=0.25 falls in segment [0..1] between stops[0] and stops[1], frac=0.5
    const c = interpolateStops(stops, 0.25);
    expect(c.r).toBeCloseTo(127.5, 5);
    expect(c.g).toBeCloseTo(0, 5);
    expect(c.b).toBeCloseTo(0, 5);
  });
});
