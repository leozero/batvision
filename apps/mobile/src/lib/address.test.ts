import { describe, expect, it } from "@jest/globals";
import { isPrivateHost, isPublicCleartext, normalizeServer } from "./address";

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

describe("isPrivateHost", () => {
  it("knows this device and the local network", () => {
    for (const host of [
      "localhost",
      "127.0.0.1",
      "10.0.2.2",
      "172.16.0.1",
      "172.31.255.254",
      "192.168.1.20",
      "169.254.10.1",
      "100.100.1.2",
      "nas",
      "nas.local",
      "NAS.Local.",
      "[::1]",
      "[fe80::1]",
      "[fd12:3456::1]",
      "[::ffff:192.168.1.20]",
    ])
      expect([host, isPrivateHost(host)]).toEqual([host, true]);
  });

  it("takes everything else for the internet", () => {
    for (const host of [
      "media.example.fr",
      "8.8.8.8",
      "172.32.0.1",
      "172.15.0.1",
      "192.169.1.1",
      "100.128.0.1",
      "10.example.com",
      "local.example.com",
      "[2001:db8::1]",
      "[::ffff:8.8.8.8]",
    ])
      expect([host, isPrivateHost(host)]).toEqual([host, false]);
  });
});

describe("isPublicCleartext", () => {
  it("is true only for http:// to a public host", () => {
    expect(isPublicCleartext("http://media.example.fr")).toBe(true);
    expect(isPublicCleartext(" HTTP://Media.Example.fr:8420/web ")).toBe(true);
    expect(isPublicCleartext("http://[2001:db8::1]:8420")).toBe(true);
  });

  it("is false for https, local servers and unfinished addresses", () => {
    expect(isPublicCleartext("https://media.example.fr")).toBe(false);
    expect(isPublicCleartext("media.example.fr")).toBe(false);
    expect(isPublicCleartext("http://192.168.1.20:8420")).toBe(false);
    expect(isPublicCleartext("http://nas.local:8420")).toBe(false);
    expect(isPublicCleartext("http://[fe80::1]:8420")).toBe(false);
    expect(isPublicCleartext("http://")).toBe(false);
    expect(isPublicCleartext("")).toBe(false);
  });
});
