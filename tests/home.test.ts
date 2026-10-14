import { describe, expect, it } from "vitest";
import { ratingProgress } from "../src/Home";

describe("Home rating goal", () => {
  it("hides the bar through 1900 and without a rating", () => {
    for (const rating of [null, 0, 1000, 1899, 1900, NaN, Infinity]) {
      expect(ratingProgress(rating)).toBeNull();
    }
  });
  it("shows only the last hundred points to 2000", () => {
    expect(ratingProgress(1901)).toBe(0.01);
    expect(ratingProgress(1950)).toBe(0.5);
    expect(ratingProgress(1999)).toBe(0.99);
    expect(ratingProgress(2000)).toBe(1);
    expect(ratingProgress(2100)).toBe(1);
  });
});
