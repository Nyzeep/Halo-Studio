import { z } from "zod";
import { jsonValueSchema, type JsonValue } from "@halo-studio/contracts";

export const PI_VERSION = "0.81.1" as const;

const jsonValue = jsonValueSchema as z.ZodType<JsonValue>;
const piRequestIdSchema = z.string().min(1).max(512);
const piMessageTextSchema = z.string().min(1).max(32_768);

/**
 * Only these Pi RPC requests are used by Halo Studio. Keeping the wire schema
 * closed prevents a caller from smuggling session paths, shell commands, or
 * other native RPC operations through the JSONL transport.
 */
export const piCommandSchema = z.discriminatedUnion("type", [
  z.object({
    type: z.literal("prompt"),
    id: piRequestIdSchema.optional(),
    message: z.string(),
    images: z.array(z.record(z.string(), jsonValue)).optional(),
  }).strict(),
  z.object({
    type: z.literal("steer"),
    id: piRequestIdSchema.optional(),
    message: z.string(),
    images: z.array(z.record(z.string(), jsonValue)).optional(),
  }).strict(),
  z.object({
    type: z.literal("abort"),
    id: piRequestIdSchema.optional(),
  }).strict(),
  z.object({
    type: z.literal("get_state"),
    id: piRequestIdSchema.optional(),
  }).strict(),
  z.object({
    type: z.literal("new_session"),
    id: piRequestIdSchema.optional(),
  }).strict(),
  z.object({
    type: z.literal("get_messages"),
    id: piRequestIdSchema.optional(),
  }).strict(),
  z.object({
    type: z.literal("get_commands"),
    id: piRequestIdSchema.optional(),
  }).strict(),
]);
export type PiCommand = z.infer<typeof piCommandSchema>;

export const piSessionCommandSchema = z.discriminatedUnion("type", [
  z.object({ type: z.literal("get_state"), id: piRequestIdSchema.optional() }).strict(),
  z.object({ type: z.literal("new_session"), id: piRequestIdSchema.optional() }).strict(),
  z.object({ type: z.literal("get_messages"), id: piRequestIdSchema.optional() }).strict(),
  z.object({ type: z.literal("get_commands"), id: piRequestIdSchema.optional() }).strict(),
]);
export type PiSessionCommand = z.infer<typeof piSessionCommandSchema>;

export const piResponseSchema = z.object({
  type: z.literal("response"),
  id: z.string().min(1).optional(),
  command: z.string().min(1),
  success: z.boolean(),
  data: jsonValue.optional(),
  error: jsonValue.optional(),
}).passthrough();
export type PiResponse = z.infer<typeof piResponseSchema>;

export const piEventSchema = z.object({
  type: z.string().min(1),
  data: jsonValue.optional(),
}).passthrough();
export type PiEvent = z.infer<typeof piEventSchema>;

/**
 * The minimal session state Halo Studio needs. Pi's model, path, and global
 * queue configuration are deliberately not passed through this adapter.
 */
export const piSessionStateSchema = z.object({
  sessionId: piRequestIdSchema,
  sessionName: z.string().max(512).optional(),
  isStreaming: z.boolean(),
  isCompacting: z.boolean(),
  messageCount: z.number().int().nonnegative().max(1_000_000),
  pendingMessageCount: z.number().int().nonnegative().max(1_000_000),
}).strip();
export type PiSessionState = z.output<typeof piSessionStateSchema>;

export const piNewSessionResultSchema = z.object({
  cancelled: z.boolean(),
}).strip();
export type PiNewSessionResult = z.output<typeof piNewSessionResultSchema>;

export const piSessionMessageSchema = z.object({
  role: z.enum(["user", "assistant", "system"]),
  text: piMessageTextSchema,
}).strict();
export type PiSessionMessage = z.output<typeof piSessionMessageSchema>;

const piRawSessionMessagesSchema = z.object({
  messages: z.array(z.object({
    role: z.string().min(1).max(64),
  }).passthrough()).max(512),
}).strip();

const piTextContentBlockSchema = z.object({
  type: z.literal("text"),
  text: piMessageTextSchema,
}).passthrough();

/**
 * Turn Pi's heterogeneous native message history into a bounded text-only
 * projection. Tool results, custom entries, and other non-conversation data
 * are intentionally dropped before they can cross into the desktop layer.
 */
export function parsePiSessionMessages(value: unknown): PiSessionMessage[] | undefined {
  const parsed = piRawSessionMessagesSchema.safeParse(value);
  if (!parsed.success) return undefined;

  const messages: PiSessionMessage[] = [];
  for (const message of parsed.data.messages) {
    if (message.role !== "user" && message.role !== "assistant" && message.role !== "system") continue;
    const content = message.content;
    let text: string | undefined;
    if (typeof content === "string") {
      const parsedText = piMessageTextSchema.safeParse(content);
      if (!parsedText.success) return undefined;
      text = parsedText.data;
    } else if (Array.isArray(content)) {
      if (content.length > 128) return undefined;
      const parts: string[] = [];
      for (const block of content) {
        if (typeof block !== "object" || block === null || Array.isArray(block)) continue;
        if ((block as { readonly type?: unknown }).type !== "text") continue;
        const parsedBlock = piTextContentBlockSchema.safeParse(block);
        if (!parsedBlock.success) return undefined;
        parts.push(parsedBlock.data.text);
      }
      if (parts.length === 0) continue;
      text = parts.join("\n\n");
      if (!piMessageTextSchema.safeParse(text).success) return undefined;
    } else {
      return undefined;
    }
    messages.push({ role: message.role, text });
  }
  return messages;
}

const piCommandNameSchema = z.string().min(1).max(256).regex(/^[^\s/]+$/u);
export const piSessionCommandDescriptorSchema = z.object({
  name: piCommandNameSchema,
  description: z.string().min(1).max(2_048).optional(),
  source: z.enum(["extension", "prompt", "skill"]),
}).strip();
export type PiSessionCommandDescriptor = z.output<typeof piSessionCommandDescriptorSchema>;

export const piSessionCommandsSchema = z.object({
  commands: z.array(piSessionCommandDescriptorSchema).max(512),
}).strip();

function piSessionResponseSchema<TData extends z.ZodTypeAny>(command: PiSessionCommand["type"], data: TData) {
  return z.object({
    type: z.literal("response"),
    id: piRequestIdSchema,
    command: z.literal(command),
    success: z.literal(true),
    data,
  }).strip();
}

export const piSessionStateResponseSchema = piSessionResponseSchema("get_state", piSessionStateSchema);
export const piNewSessionResponseSchema = piSessionResponseSchema("new_session", piNewSessionResultSchema);
export const piSessionMessagesResponseSchema = piSessionResponseSchema("get_messages", piRawSessionMessagesSchema);
export const piSessionCommandsResponseSchema = piSessionResponseSchema("get_commands", piSessionCommandsSchema);

export type PiLifecycleState = "unavailable" | "detected" | "starting" | "ready" | "stopping" | "stopped" | "crashed";

/** A fully resolved command that can be spawned with `shell: false`. */
export interface PiLaunchTarget {
  readonly executable: string;
  readonly argvPrefix: readonly string[];
  /** A non-executed discovery anchor, such as npm's canonical `pi.cmd`. */
  readonly displayPath?: string;
}

export interface PiDetection {
  readonly status: "detected" | "unavailable";
  readonly source: "system" | "managed";
  readonly executable?: string;
  /** Present when Pi needs a verified interpreter invocation. */
  readonly launch?: PiLaunchTarget;
  readonly version?: string;
  readonly managedInstall?: "available";
}
