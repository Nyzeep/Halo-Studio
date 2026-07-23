import { z } from "zod";
import { jsonValueSchema, type JsonValue } from "@halo-studio/contracts";

export const PI_VERSION = "0.81.1" as const;

const jsonValue = jsonValueSchema as z.ZodType<JsonValue>;

export const piCommandSchema = z.object({
  type: z.enum(["prompt", "steer", "abort", "get_state"]),
  id: z.string().min(1).optional(),
  message: z.string().optional(),
  images: z.array(z.record(z.string(), jsonValue)).optional(),
}).passthrough().superRefine((command, ctx) => {
  if ((command.type === "prompt" || command.type === "steer") && typeof command.message !== "string") {
    ctx.addIssue({ code: z.ZodIssueCode.custom, path: ["message"], message: "message is required" });
  }
});
export type PiCommand = z.infer<typeof piCommandSchema>;

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

export type PiLifecycleState = "unavailable" | "detected" | "starting" | "ready" | "stopping" | "stopped" | "crashed";

export interface PiDetection {
  readonly status: "detected" | "unavailable";
  readonly source: "system" | "managed";
  readonly executable?: string;
  readonly version?: string;
  readonly managedInstall?: "available";
}
