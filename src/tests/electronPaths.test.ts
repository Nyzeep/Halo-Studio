import { describe, expect, it } from "vitest";
import { getPreloadPath } from "../main/electronPaths";

describe("Electron preload path", () => {
  it("uses an .mjs preload file so Electron can load the ESM bridge", () => {
    expect(getPreloadPath("D:\\Halo Studio\\dist\\main")).toBe("D:\\Halo Studio\\dist\\main\\preload.mjs");
  });
});
