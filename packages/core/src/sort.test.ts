import { describe, expect, it } from "vitest";
import { directionLabelKey, flipDirection, naturalDirection, parseSort } from "./sort";

describe("sort", () => {
  it("has a natural direction per sort", () => {
    expect(naturalDirection("title")).toBe("asc");
    expect(naturalDirection("added")).toBe("desc");
    expect(naturalDirection("year")).toBe("desc");
    expect(naturalDirection("rating")).toBe("desc");
  });

  it("flips", () => {
    expect(flipDirection("asc")).toBe("desc");
    expect(flipDirection("desc")).toBe("asc");
  });

  it("parses what was remembered", () => {
    expect(parseSort(null, null)).toEqual({ sort: "added", direction: "desc" });
    expect(parseSort("title", null)).toEqual({ sort: "title", direction: "asc" });
    expect(parseSort("year", "asc")).toEqual({ sort: "year", direction: "asc" });
    expect(parseSort("bogus", "sideways")).toEqual({ sort: "added", direction: "desc" });
  });

  it("names the label", () => {
    expect(directionLabelKey("title", "desc")).toBe("browse.direction.title.desc");
  });
});
