import { describe, expect, it, vi } from "vitest";

import { buildRuntimeEnvironment } from "./environment.js";

describe("runtime environment allowlist", () => {
  const invalidEnvironmentError = {
    code: "ProtocolViolation",
    message: "Runtime environment input is not permitted.",
  } as const;

  it("copies only audited host variables and omits undefined values", () => {
    const result = buildRuntimeEnvironment({
      ALL_PROXY: "socks://proxy",
      HOME: "/home/user",
      LANG: "en_US.UTF-8",
      LC_ALL: undefined,
      NO_PROXY: "localhost",
      PATH: "/usr/bin",
      TEMP: "/tmp",
      TMPDIR: "/var/tmp",
      USERPROFILE: "C:\\Users\\user",
      all_proxy: "socks://lower-proxy",
      http_proxy: "http://lower-proxy",
    });

    expect(result).toEqual({
      PATH: "/usr/bin",
      HOME: "/home/user",
      USERPROFILE: "C:\\Users\\user",
      TEMP: "/tmp",
      TMPDIR: "/var/tmp",
      LANG: "en_US.UTF-8",
      ALL_PROXY: "socks://proxy",
      NO_PROXY: "localhost",
      http_proxy: "http://lower-proxy",
      all_proxy: "socks://lower-proxy",
    });
    expect(Object.values(result)).not.toContain(undefined);
  });

  it("normalizes Windows Path to a deterministic PATH key", () => {
    expect(buildRuntimeEnvironment({ Path: "C:\\Windows" })).toEqual({
      PATH: "C:\\Windows",
    });
    expect(
      buildRuntimeEnvironment({ PATH: "preferred", Path: "fallback" }),
    ).toEqual({ PATH: "preferred" });
  });

  it("never inherits credentials, startup controls, or random host variables", () => {
    const result = buildRuntimeEnvironment({
      API_KEY: "host-api-canary",
      ELECTRON_RUN_AS_NODE: "1",
      LD_PRELOAD: "/host/library.so",
      NODE_OPTIONS: "--require host-hook.js",
      OPENAI_API_KEY: "host-provider-canary",
      PATH: "/usr/bin",
      RANDOM_VALUE: "host-random-canary",
      TOKEN: "host-token-canary",
    });

    expect(result).toEqual({ PATH: "/usr/bin" });
    expect(JSON.stringify(result)).not.toContain("canary");
  });

  it("injects provider credentials only from explicit provider values", () => {
    const hostEnvironment = {
      ANTHROPIC_API_KEY: "host-secret-canary",
      PATH: "/usr/bin",
    };
    const providerEnvironment = {
      ANTHROPIC_API_KEY: "explicit-provider-value",
      OPENAI_API_KEY: "explicit-openai-value",
    };

    const result = buildRuntimeEnvironment(hostEnvironment, providerEnvironment);

    expect(result).toEqual({ PATH: "/usr/bin", ...providerEnvironment });
    expect(result.ANTHROPIC_API_KEY).not.toBe(hostEnvironment.ANTHROPIC_API_KEY);
  });

  it("returns a fresh object without mutating either input", () => {
    const host = { PATH: "/bin" };
    const provider = { OPENAI_API_KEY: "explicit" };
    const result = buildRuntimeEnvironment(host, provider);

    result.PATH = "/changed";

    expect(host).toEqual({ PATH: "/bin" });
    expect(provider).toEqual({ OPENAI_API_KEY: "explicit" });
    expect(result).not.toBe(host);
    expect(result).not.toBe(provider);
  });

  it.each([
    "lowercase_key",
    "1INVALID",
    "BAD-NAME",
    "NODE_OPTIONS",
    "ELECTRON_RUN_AS_NODE",
    "LD_PRELOAD",
    "LD_LIBRARY_PATH",
    "DYLD_INSERT_LIBRARIES",
    "NODE_PATH",
    "PYTHONPATH",
    "BASH_ENV",
    "OPENCODE_DISABLE_PROJECT_CONFIG",
    "PATH",
    "HOME",
    "HTTPS_PROXY",
  ])("rejects unsafe provider variable %s", (name) => {
    expect(() =>
      buildRuntimeEnvironment({}, { [name]: "provider-secret-canary" }),
    ).toThrowError(
      expect.objectContaining({
        code: "ProtocolViolation",
      }),
    );
  });

  it("rejects non-string provider values without including them in errors", () => {
    const secret = "undefined-secret-canary";
    const provider = { OPENAI_API_KEY: undefined } as unknown as Record<
      string,
      string
    >;

    let thrown: unknown;
    try {
      buildRuntimeEnvironment({ RANDOM: secret }, provider);
    } catch (error) {
      thrown = error;
    }

    expect(thrown).toMatchObject({ code: "ProtocolViolation" });
    expect(String(thrown)).not.toContain(secret);
  });

  it("does not echo an invalid provider key in its error message", () => {
    const invalidKey = "bad-provider-key-canary-secret";

    let thrown: unknown;
    try {
      buildRuntimeEnvironment({}, { [invalidKey]: "credential-canary-secret" });
    } catch (error) {
      thrown = error;
    }

    expect(thrown).toMatchObject({ code: "ProtocolViolation" });
    expect(String(thrown)).not.toContain("canary-secret");
  });

  it("rejects provider values containing a NUL character", () => {
    expect(() =>
      buildRuntimeEnvironment(
        {},
        { OPENAI_API_KEY: "credential-canary\0secret" },
      ),
    ).toThrowError(expect.objectContaining({ code: "ProtocolViolation" }));
  });

  it("rejects a host accessor without executing its getter", () => {
    const getter = vi.fn(() => {
      throw new Error("host-getter-canary-secret");
    });
    const host = Object.defineProperty({}, "PATH", {
      enumerable: true,
      get: getter,
    });

    let thrown: unknown;
    try {
      buildRuntimeEnvironment(host);
    } catch (error) {
      thrown = error;
    }

    expect(thrown).toMatchObject(invalidEnvironmentError);
    expect(getter).not.toHaveBeenCalled();
    expect(String(thrown)).not.toContain("canary-secret");
  });

  it("rejects a host Proxy without executing its traps", () => {
    const get = vi.fn(() => {
      throw new Error("host-proxy-get-canary-secret");
    });
    const getOwnPropertyDescriptor = vi.fn(() => {
      throw new Error("host-proxy-descriptor-canary-secret");
    });
    const host = new Proxy({}, { get, getOwnPropertyDescriptor });

    let thrown: unknown;
    try {
      buildRuntimeEnvironment(host);
    } catch (error) {
      thrown = error;
    }

    expect(thrown).toMatchObject(invalidEnvironmentError);
    expect(get).not.toHaveBeenCalled();
    expect(getOwnPropertyDescriptor).not.toHaveBeenCalled();
    expect(String(thrown)).not.toContain("canary-secret");
  });

  it("rejects a provider accessor without executing its getter", () => {
    const getter = vi.fn(() => {
      throw new Error("provider-getter-canary-secret");
    });
    const provider = Object.defineProperty({}, "OPENAI_API_KEY", {
      enumerable: true,
      get: getter,
    }) as Record<string, string>;

    let thrown: unknown;
    try {
      buildRuntimeEnvironment({}, provider);
    } catch (error) {
      thrown = error;
    }

    expect(thrown).toMatchObject(invalidEnvironmentError);
    expect(getter).not.toHaveBeenCalled();
    expect(String(thrown)).not.toContain("canary-secret");
  });

  it("rejects a provider Proxy without executing enumeration traps", () => {
    const ownKeys = vi.fn(() => {
      throw new Error("provider-own-keys-canary-secret");
    });
    const getOwnPropertyDescriptor = vi.fn(() => {
      throw new Error("provider-descriptor-canary-secret");
    });
    const provider = new Proxy({}, { ownKeys, getOwnPropertyDescriptor });

    let thrown: unknown;
    try {
      buildRuntimeEnvironment({}, provider);
    } catch (error) {
      thrown = error;
    }

    expect(thrown).toMatchObject(invalidEnvironmentError);
    expect(ownKeys).not.toHaveBeenCalled();
    expect(getOwnPropertyDescriptor).not.toHaveBeenCalled();
    expect(String(thrown)).not.toContain("canary-secret");
  });

  it.each(["PATH", "HOME", "HTTPS_PROXY", "http_proxy"])(
    "rejects NUL in host variable %s",
    (name) => {
      let thrown: unknown;
      try {
        buildRuntimeEnvironment({ [name]: "host-canary\0secret" });
      } catch (error) {
        thrown = error;
      }

      expect(thrown).toMatchObject(invalidEnvironmentError);
      expect(String(thrown)).not.toContain("canary");
    },
  );
});
