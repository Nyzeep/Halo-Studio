import { describe, expect, it } from "vitest";

import type { CredentialVault } from "@halo-studio/storage";

import {
  createEnvironmentPiLaunchResolver,
  validatePiLaunchConfiguration,
} from "../src/main/piLaunchResolver.js";

function vault(values: Readonly<Record<string, string | null>>, available = true): CredentialVault {
  return {
    isAvailable: () => available,
    get: async (reference) => values[reference] ?? null,
    store: async () => undefined,
    delete: async () => undefined,
  };
}

const workspace = {
  id: "a".repeat(64),
  realPath: "C:\\workspace",
  trustState: "trusted" as const,
};

describe("Pi managed launch configuration", () => {
  it("resolves selectors and a vault value only inside Main-owned configuration", async () => {
    const secret = "pi-launch-canary";
    const resolver = createEnvironmentPiLaunchResolver({
      environment: {
        HALO_PI_MODEL: "test-model",
        HALO_PI_THINKING: "medium",
        HALO_PI_PROVIDER_ENV_KEY: "OPENAI_API_KEY",
        HALO_PI_CREDENTIAL_REFERENCE: "provider:primary",
      },
      vault: vault({ "provider:primary": secret }),
    });

    const launch = await resolver({ workspace });

    expect(launch.model).toBe("test-model");
    expect(launch.thinking).toBe("medium");
    expect(launch.providerEnvironment).toEqual({ OPENAI_API_KEY: secret });
    expect(launch.allowedProviderKeys).toEqual(new Set(["OPENAI_API_KEY"]));
  });

  it("fails closed when selectors or protected credentials are unavailable", async () => {
    const base = {
      HALO_PI_MODEL: "test-model",
      HALO_PI_THINKING: "medium",
      HALO_PI_PROVIDER_ENV_KEY: "OPENAI_API_KEY",
      HALO_PI_CREDENTIAL_REFERENCE: "provider:primary",
    } as const;

    await expect(createEnvironmentPiLaunchResolver({
      environment: { ...base, HALO_PI_THINKING: "" },
      vault: vault({ "provider:primary": "secret" }),
    })({ workspace })).rejects.toMatchObject({ code: "RuntimeUnavailable" });
    await expect(createEnvironmentPiLaunchResolver({
      environment: base,
      vault: vault({ "provider:primary": null }),
    })({ workspace })).rejects.toMatchObject({ code: "RuntimeUnavailable" });
    await expect(createEnvironmentPiLaunchResolver({
      environment: base,
      vault: vault({ "provider:primary": "secret" }, false),
    })({ workspace })).rejects.toMatchObject({ code: "RuntimeUnavailable" });
  });

  it("rejects proxy, accessor, and unsafe provider configuration values", () => {
    const accessor = Object.create(null) as Record<string, unknown>;
    Object.defineProperties(accessor, {
      model: { enumerable: true, get: () => "test-model" },
      thinking: { enumerable: true, value: "medium" },
      providerEnvironment: { enumerable: true, value: {} },
      allowedProviderKeys: { enumerable: true, value: new Set<string>() },
    });
    const proxied = new Proxy({
      model: "test-model",
      thinking: "medium",
      providerEnvironment: {},
      allowedProviderKeys: new Set<string>(),
    }, {});

    expect(() => validatePiLaunchConfiguration(accessor)).toThrowError(expect.objectContaining({
      code: "RuntimeUnavailable",
    }));
    expect(() => validatePiLaunchConfiguration(proxied)).toThrowError(expect.objectContaining({
      code: "RuntimeUnavailable",
    }));
    expect(() => validatePiLaunchConfiguration({
      model: "test-model",
      thinking: "medium",
      providerEnvironment: { NODE_OPTIONS: "--inspect" },
      allowedProviderKeys: new Set(["NODE_OPTIONS"]),
    })).toThrowError(expect.objectContaining({ code: "RuntimeUnavailable" }));
  });
});
