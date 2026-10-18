import { describe, expect, it } from "vitest";
import { controlLabel } from "../src/control";
describe("control labels retain the native query value", () => {
  it("shows minutes and preserves fractional increments", () => {
    expect(controlLabel("600+2")).toBe("10+2");
    expect(controlLabel("30+0.5")).toBe("0,5+0.5");
    expect(controlLabel("unknown")).toBe("Неизвестен");
  });
});
