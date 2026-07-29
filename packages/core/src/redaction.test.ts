import { describe, expect, it, vi } from "vitest";

import {
  REDACTED,
  TRUNCATED,
  UNSERIALIZABLE,
  redactLogValue,
} from "./redaction.js";

describe("log redaction", () => {
  it("redacts sensitive keys recursively without retaining secret fragments", () => {
    const canaries = [
      "authorization-canary-secret",
      "api-key-canary-secret",
      "token-canary-secret",
      "password-canary-secret",
      "cookie-canary-secret",
    ];
    const value = {
      headers: {
        Authorization: canaries[0],
        "Set-Cookie": canaries[4],
        Accept: "application/json",
      },
      accounts: [
        {
          api_key: canaries[1],
          accessToken: canaries[2],
          passwordHash: canaries[3],
          visible: true,
        },
      ],
    };

    const result = redactLogValue(value);
    const serialized = JSON.stringify(result);

    expect(result).toEqual({
      headers: {
        Authorization: REDACTED,
        "Set-Cookie": REDACTED,
        Accept: "application/json",
      },
      accounts: [
        {
          api_key: REDACTED,
          accessToken: REDACTED,
          passwordHash: REDACTED,
          visible: true,
        },
      ],
    });
    for (const canary of canaries) {
      expect(serialized).not.toContain(canary);
    }
  });

  it("truncates long strings to a caller-controlled bound", () => {
    const result = redactLogValue("abcdefghijklmno", { maxStringLength: 8 });

    expect(result).toBe(`abcdefgh${TRUNCATED}`);
    expect((result as string).length).toBeLessThan(32);
  });

  it("bounds object property names as part of the output size limit", () => {
    const longKey = "k".repeat(128);

    const result = redactLogValue({ [longKey]: "visible" }, {
      maxStringLength: 8,
    });

    expect(result).toEqual({ [`kkkkkkkk${TRUNCATED}`]: "visible" });
    expect(JSON.stringify(result).length).toBeLessThan(64);
  });

  it("replaces circular references with an explicit placeholder", () => {
    const cyclic: Record<string, unknown> = { label: "root" };
    cyclic.self = cyclic;

    const result = redactLogValue(cyclic);

    expect(result).toEqual({ label: "root", self: UNSERIALIZABLE });
    expect(() => JSON.stringify(result)).not.toThrow();
  });

  it("handles Date, Error, BigInt, function, and invalid numbers safely", () => {
    const result = redactLogValue({
      date: new Date("2026-07-22T00:00:00.000Z"),
      error: new TypeError("invalid input"),
      bigint: 42n,
      fn: () => "must not run",
      nan: Number.NaN,
      infinity: Number.POSITIVE_INFINITY,
      missing: undefined,
    });

    expect(result).toEqual({
      date: "2026-07-22T00:00:00.000Z",
      error: { name: "TypeError", message: "invalid input" },
      bigint: "42n",
      fn: UNSERIALIZABLE,
      nan: UNSERIALIZABLE,
      infinity: UNSERIALIZABLE,
      missing: UNSERIALIZABLE,
    });
    expect(() => JSON.stringify(result)).not.toThrow();
  });

  it("uses Date intrinsics instead of executing instance overrides", () => {
    const date = new Date("2026-07-22T00:00:00.000Z");
    const overridden = vi.fn(() => "date-canary-secret");
    date.toISOString = overridden;

    const result = redactLogValue(date);

    expect(result).toBe("2026-07-22T00:00:00.000Z");
    expect(overridden).not.toHaveBeenCalled();
    expect(JSON.stringify(result)).not.toContain("date-canary-secret");
  });

  it("does not execute traps on a native Error prototype Proxy", () => {
    const getOwnPropertyDescriptor = vi.fn(() => {
      throw new Error("error-prototype-descriptor-canary-secret");
    });
    const getPrototypeOf = vi.fn(() => {
      throw new Error("error-prototype-chain-canary-secret");
    });
    const proxyPrototype = new Proxy(
      {},
      { getOwnPropertyDescriptor, getPrototypeOf },
    );
    const error = new Error("visible message");
    Object.setPrototypeOf(error, proxyPrototype);

    const result = redactLogValue(error);

    expect(getOwnPropertyDescriptor).not.toHaveBeenCalled();
    expect(getPrototypeOf).not.toHaveBeenCalled();
    expect(result).toEqual({
      name: UNSERIALIZABLE,
      message: "visible message",
    });
    expect(JSON.stringify(result)).not.toContain("canary-secret");
  });

  it("does not execute accessors while inspecting objects", () => {
    const getter = vi.fn(() => {
      throw new Error("getter-canary-secret");
    });
    const value = Object.defineProperty({ ordinary: "visible" }, "computed", {
      enumerable: true,
      get: getter,
    });

    const result = redactLogValue(value);

    expect(result).toEqual({ ordinary: "visible", computed: UNSERIALIZABLE });
    expect(getter).not.toHaveBeenCalled();
    expect(JSON.stringify(result)).not.toContain("getter-canary-secret");
  });

  it("redacts a sensitive accessor without touching its getter", () => {
    const getter = vi.fn(() => "accessor-token-canary-secret");
    const value = Object.defineProperty({}, "refreshToken", {
      enumerable: true,
      get: getter,
    });

    const result = redactLogValue(value);

    expect(result).toEqual({ refreshToken: REDACTED });
    expect(getter).not.toHaveBeenCalled();
    expect(JSON.stringify(result)).not.toContain("accessor-token-canary-secret");
  });

  it("returns a placeholder for proxies that cannot be inspected", () => {
    const ownKeys = vi.fn(() => {
      throw new Error("proxy-canary-secret");
    });
    const proxy = new Proxy(
      {},
      {
        ownKeys,
      },
    );

    const result = redactLogValue(proxy);

    expect(result).toBe(UNSERIALIZABLE);
    expect(ownKeys).not.toHaveBeenCalled();
    expect(JSON.stringify(result)).not.toContain("proxy-canary-secret");
  });

  it("does not execute Proxy prototype traps while classifying objects", () => {
    const getPrototypeOf = vi.fn(() => {
      throw new Error("prototype-proxy-canary-secret");
    });
    const proxyPrototype = new Proxy({}, { getPrototypeOf });
    const value = Object.create(proxyPrototype) as object;

    const result = redactLogValue(value);

    expect(result).toBe(UNSERIALIZABLE);
    expect(getPrototypeOf).not.toHaveBeenCalled();
    expect(JSON.stringify(result)).not.toContain("prototype-proxy-canary-secret");
  });

  it("does not let exceptional options escape the redaction boundary", () => {
    const options = Object.defineProperty({}, "maxDepth", {
      get() {
        throw new Error("option-canary-secret");
      },
    });

    const result = redactLogValue(
      { token: "credential-canary-secret" },
      options,
    );

    expect(result).toBe(UNSERIALIZABLE);
    expect(JSON.stringify(result)).not.toContain("canary-secret");
  });

  it("bounds depth, node count, and container entries", () => {
    const result = redactLogValue(
      {
        first: { nested: { tooDeep: "depth-canary-secret" } },
        second: [1, 2, 3, 4, 5],
        third: { a: 1, b: 2, c: 3 },
      },
      {
        maxContainerEntries: 2,
        maxDepth: 2,
        maxNodes: 8,
        maxStringLength: 32,
      },
    );
    const serialized = JSON.stringify(result);

    expect(serialized).toContain(TRUNCATED);
    expect(serialized).not.toContain("depth-canary-secret");
    expect(serialized.length).toBeLessThan(512);
  });

  it("bounds large plain objects without materializing Reflect own-key lists", () => {
    const value = Object.fromEntries(
      Array.from({ length: 5_000 }, (_, index) => [`key${index}`, index]),
    );
    const ownKeys = vi
      .spyOn(Reflect, "ownKeys")
      .mockImplementation(() => {
        throw new Error("unbounded-own-keys-canary-secret");
      });

    try {
      const result = redactLogValue(value, { maxContainerEntries: 3 });

      expect(result).toEqual({
        key0: 0,
        key1: 1,
        key2: 2,
        [TRUNCATED]: TRUNCATED,
      });
      expect(JSON.stringify(result)).not.toContain("canary-secret");
    } finally {
      ownKeys.mockRestore();
    }
  });

  it("handles invalid dates and unsupported object instances", () => {
    class Unsupported {
      value = "instance-canary-secret";
    }

    const result = redactLogValue({
      invalidDate: new Date(Number.NaN),
      unsupported: new Unsupported(),
    });

    expect(result).toEqual({
      invalidDate: UNSERIALIZABLE,
      unsupported: UNSERIALIZABLE,
    });
    expect(JSON.stringify(result)).not.toContain("instance-canary-secret");
  });
});
