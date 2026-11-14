import { describe, expect, it } from "vitest";
import { cueAt, parseWebVtt } from "./subtitles";

const VTT = `WEBVTT

1
00:00:00.500 --> 00:00:05.000 align:center
Bonjour <i>depuis</i> le disque

00:01:02.250 --> 01:00:03.000
Ligne un
Ligne &amp; deux
`;

describe("parseWebVtt", () => {
  it("reads cues with or without ids, hours and settings", () => {
    expect(parseWebVtt(VTT)).toEqual([
      { start: 0.5, end: 5, text: "Bonjour depuis le disque" },
      { start: 62.25, end: 3603, text: "Ligne un\nLigne & deux" },
    ]);
  });

  it("ignores the header, notes and broken cues", () => {
    expect(
      parseWebVtt("WEBVTT\n\nNOTE hi\n\nxx --> yy\nnope\n\n00:01.000 --> 00:02.000\n"),
    ).toEqual([]);
  });

  it("finds the text at a time", () => {
    const cues = parseWebVtt(VTT);
    expect(cueAt(cues, 1)).toBe("Bonjour depuis le disque");
    expect(cueAt(cues, 5)).toBeNull();
    expect(cueAt(cues, 70)).toBe("Ligne un\nLigne & deux");
  });
});
