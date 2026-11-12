import { beforeEach, describe, expect, it } from "vitest";
import { readAutoLevel, readQualityMode, saveAutoLevel, saveQualityMode } from "./quality";

describe("stored choice", () => {
  beforeEach(() => localStorage.clear());

  it("defaults to auto, starting at max", () => {
    expect(readQualityMode()).toBe("auto");
    expect(readAutoLevel()).toBe("max");
  });

  it("is remembered on the device", () => {
    saveQualityMode("low");
    saveAutoLevel("sos");
    expect(readQualityMode()).toBe("low");
    expect(readAutoLevel()).toBe("sos");
    localStorage.setItem("bv.quality", "nonsense");
    expect(readQualityMode()).toBe("auto");
  });
});
