import { z } from "zod";

import { agentKindSchema, workspaceIdSchema } from "./agent.js";
import { agentEventEnvelopeSchema } from "./events.js";

const boundedTextSchema = z.string().min(1).max(32_768);

/**
 * Opaque native identifier. It is never interpreted as a path or command by
 * Renderer, Preload, or the generic IPC layer.
 */
export const sessionIdSchema = z.string().min(1).max(512);
export type SessionId = z.infer<typeof sessionIdSchema>;

export const sessionSummarySchema = z
  .object({
    agentKind: agentKindSchema,
    sessionId: sessionIdSchema,
    title: boundedTextSchema.optional(),
    updatedAt: z.string().datetime({ offset: true }).optional(),
    active: z.boolean(),
  })
  .strict();
export type SessionSummary = z.infer<typeof sessionSummarySchema>;

export const sessionMessageRoleSchema = z.enum([
  "user",
  "assistant",
  "system",
  "unknown",
]);
export type SessionMessageRole = z.infer<typeof sessionMessageRoleSchema>;

/**
 * A bounded rendering projection of native agent history. Native message
 * payloads stay inside Main; callers cannot treat this as an editing model.
 */
export const sessionMessageSchema = z
  .object({
    agentKind: agentKindSchema,
    sessionId: sessionIdSchema,
    ordinal: z.number().int().nonnegative(),
    role: sessionMessageRoleSchema,
    text: boundedTextSchema,
  })
  .strict();
export type SessionMessage = z.infer<typeof sessionMessageSchema>;

export const sessionHistorySchema = z
  .object({
    session: sessionSummarySchema,
    messages: z.array(sessionMessageSchema).max(512),
  })
  .strict();
export type SessionHistory = z.infer<typeof sessionHistorySchema>;

export const sessionSendResultSchema = z
  .object({
    session: sessionSummarySchema,
    clientRequestId: z.string().uuid(),
    accepted: z.literal(true),
  })
  .strict();
export type SessionSendResult = z.infer<typeof sessionSendResultSchema>;

export const sessionSnapshotRequestSchema = z
  .object({ workspaceId: workspaceIdSchema })
  .strict();
export const sessionCreateRequestSchema = z
  .object({ workspaceId: workspaceIdSchema, agentKind: agentKindSchema })
  .strict();
export const sessionSelectRequestSchema = z
  .object({
    workspaceId: workspaceIdSchema,
    agentKind: agentKindSchema,
    sessionId: sessionIdSchema,
  })
  .strict();
export const sessionHistoryRequestSchema = sessionSelectRequestSchema;
export const sessionSendRequestSchema = sessionSelectRequestSchema.extend({
  message: boundedTextSchema,
  clientRequestId: z.string().uuid(),
}).strict();
export const sessionAbortRequestSchema = sessionSelectRequestSchema;

/** Fixed Electron push channel. It is not a general IPC subscription API. */
export const sessionEventChannel = "session.event" as const;
export const sessionEventSchema = agentEventEnvelopeSchema;
export type SessionEvent = z.infer<typeof sessionEventSchema>;
