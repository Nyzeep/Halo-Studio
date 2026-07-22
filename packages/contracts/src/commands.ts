import { z } from "zod";

import { agentKindSchema, capabilityChannelSchema } from "./agent.js";

export const commandDescriptorSchema = z
  .object({
    name: z.string().min(1),
    argumentHint: z.string().min(1).optional(),
    agentKind: agentKindSchema,
    source: z.enum(["native", "extension", "tui"]),
    channel: capabilityChannelSchema,
    allowedWhileRunning: z.boolean(),
    mutatesGlobalDefaults: z.boolean(),
    tuiOnly: z.boolean(),
  })
  .strict();
export type CommandDescriptor = z.infer<typeof commandDescriptorSchema>;
