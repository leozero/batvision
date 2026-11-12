import { describe, expect, it } from "@jest/globals";
import { normalizeServer } from "./address";

describe("normalizeServer", () => {
  it("adds https and drops paths and trailing slashes", () => {
    expect(normalizeServer("media.example.fr")).toBe("https://media.example.fr");
    expect(normalizeServer(" https://media.example.fr/web/ ")).toBe("https://media.example.fr");
  });

  it("keeps an explicit http address and its port (local servers)", () => {
    expect(normalizeServer("http://10.0.2.2:8420")).toBe("http://10.0.2.2:8420");
  });

  it("rejects what can't be an address", () => {
    expect(normalizeServer("")).toBeNull();
    expect(normalizeServer("http://")).toBeNull();
  });
});

describe("normalizeServer edge cases", () => {
  it("lowercases the host and refuses spaces", () => {
    expect(normalizeServer("HTTPS://Media.Example.fr")).toBe("https://media.example.fr");
    expect(normalizeServer("media example.fr")).toBeNull();
  });
});
