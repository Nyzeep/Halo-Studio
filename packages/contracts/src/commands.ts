import { z } from "zod";

import { agentKindSchema, capabilityChannelSchema } from "./agent.js";

export const commandDescriptorSchema = z
  .object({
    name: z.string().regex(/^\/[\S]+$/u, "Commands begin with a slash."),
    description: z.string().min(1).max(2_048).optional(),
    argumentHint: z.string().min(1).optional(),
    agentKind: agentKindSchema,
    source: z.enum(["native", "extension", "prompt", "skill", "tui"]),
    channel: capabilityChannelSchema,
    allowedWhileRunning: z.boolean(),
    mutatesGlobalDefaults: z.boolean(),
    tuiOnly: z.boolean(),
  })
  .strict();
export type CommandDescriptor = z.infer<typeof commandDescriptorSchema>;
