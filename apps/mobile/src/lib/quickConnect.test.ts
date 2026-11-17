import { describe, expect, it } from "@jest/globals";
import { approveUrl, spacedCode, typedCode } from "./quickConnect";

describe("spacedCode", () => {
  it("splits a code in two halves", () => {
    expect(spacedCode("ABC123")).toBe("ABC 123");
  });
  it("leaves other lengths alone", () => {
    expect(spacedCode("ABCD")).toBe("ABCD");
  });
});

describe("approveUrl", () => {
  it("opens the web account page with the code filled in", () => {
    expect(approveUrl("https://media.example.com", "ABC123")).toBe(
      "https://media.example.com/account?code=ABC123",
    );
  });
});

describe("typedCode", () => {
  it("accepts the code as shown, spaces and case aside", () => {
    expect(typedCode("abc 123")).toBe("ABC123");
    expect(typedCode(" ABC123 ")).toBe("ABC123");
  });
  it("refuses anything but six characters", () => {
    expect(typedCode("ABC12")).toBeNull();
    expect(typedCode("ABC1234")).toBeNull();
  });
});
