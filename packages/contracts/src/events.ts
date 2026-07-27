import { z } from "zod";

import { workspaceIdSchema } from "./agent.js";
import { optionalJsonValueSchema } from "./json.js";

export const piEventPayloadSchema = z
  .object({
    protocol: z.literal("pi-rpc"),
    type: z.string().min(1),
    data: optionalJsonValueSchema,
  })
  .strict();
export type PiEventPayload = z.infer<typeof piEventPayloadSchema>;

export const openCodeEventPayloadSchema = z
  .object({
    protocol: z.literal("opencode-sse"),
    type: z.string().min(1),
    data: optionalJsonValueSchema,
    unknown: z.boolean().optional(),
  })
  .strict();
export type OpenCodeEventPayload = z.infer<typeof openCodeEventPayloadSchema>;

const eventEnvelopeFields = {
  eventId: z.string().uuid(),
  workspaceId: workspaceIdSchema,
  sessionId: z.string().min(1).optional(),
  sequence: z.number().int().nonnegative(),
  timestamp: z.string().datetime({ offset: true }),
};

export const piAgentEventEnvelopeSchema = z
  .object({
    ...eventEnvelopeFields,
    agentKind: z.literal("pi"),
    payload: piEventPayloadSchema,
  })
  .strict();

export const openCodeAgentEventEnvelopeSchema = z
  .object({
    ...eventEnvelopeFields,
    agentKind: z.literal("opencode"),
    payload: openCodeEventPayloadSchema,
  })
  .strict();

export const agentEventEnvelopeSchema = z.discriminatedUnion("agentKind", [
  piAgentEventEnvelopeSchema,
  openCodeAgentEventEnvelopeSchema,
]);
export type AgentEventEnvelope = z.infer<typeof agentEventEnvelopeSchema>;
