import { z } from "zod";

export const agentKindSchema = z.enum(["pi", "opencode"]);
export type AgentKind = z.infer<typeof agentKindSchema>;

export const capabilityChannelSchema = z.enum([
  "rpc",
  "http",
  "sse",
  "cli",
  "native",
  "unavailable",
]);
export type CapabilityChannel = z.infer<typeof capabilityChannelSchema>;

export const capabilitySchema = z
  .object({
    supported: z.boolean(),
    channel: capabilityChannelSchema,
    restartRequired: z.boolean(),
    reason: z.string().min(1).optional(),
  })
  .strict()
  .superRefine((capability, context) => {
    if (capability.supported === (capability.channel === "unavailable")) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        message:
          "Supported capabilities require an available channel; unsupported capabilities require the unavailable channel.",
        path: ["channel"],
      });
    }
  });
export type CapabilityDescriptor = z.infer<typeof capabilitySchema>;

export const agentCapabilitiesSchema = z
  .object({
    sessions: capabilitySchema,
    streamingMessages: capabilitySchema,
    toolEvents: capabilitySchema,
    permissions: capabilitySchema,
    diff: capabilitySchema,
    commands: capabilitySchema,
    mcp: capabilitySchema,
    skills: capabilitySchema,
    prompts: capabilitySchema,
    extensions: capabilitySchema,
    packages: capabilitySchema,
    models: capabilitySchema,
    usage: capabilitySchema,
  })
  .strict();
export type AgentCapabilities = z.infer<typeof agentCapabilitiesSchema>;

export const runtimeSourceSchema = z.enum(["system", "managed", "bundled"]);
export type RuntimeSource = z.infer<typeof runtimeSourceSchema>;

export const runtimeHealthSchema = z.enum([
  "unavailable",
  "detected",
  "installed",
  "starting",
  "ready",
  "healthy",
  "stopping",
  "stopped",
  "crashed",
  "version-mismatch",
]);
export type RuntimeHealth = z.infer<typeof runtimeHealthSchema>;

export const runtimeBindingSchema = z
  .object({
    agentKind: agentKindSchema,
    source: runtimeSourceSchema,
    executable: z.string().min(1).optional(),
    version: z.string().min(1).optional(),
    health: runtimeHealthSchema,
    capabilities: agentCapabilitiesSchema,
  })
  .strict();
export type RuntimeBinding = z.infer<typeof runtimeBindingSchema>;

export const workspaceIdSchema = z
  .string()
  .regex(/^[0-9a-f]{64}$/u, "Expected a lowercase SHA-256 digest.");
export type WorkspaceId = z.infer<typeof workspaceIdSchema>;
