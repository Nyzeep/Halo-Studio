import { z } from "zod";

import { jsonValueSchema, optionalJsonValueSchema } from "./json.js";

export {
  jsonValueSchema,
  type JsonPrimitive,
  type JsonValue,
} from "./json.js";

export const appErrorCodeSchema = z.enum([
  "RuntimeUnavailable",
  "VersionMismatch",
  "AuthenticationFailed",
  "PermissionRequired",
  "WorkspaceUntrusted",
  "TransportDisconnected",
  "ProtocolViolation",
  "ConfigConflict",
  "UnsafePath",
  "MigrationFailed",
]);
export type AppErrorCode = z.infer<typeof appErrorCodeSchema>;

export const appErrorSchema = z
  .object({
    code: appErrorCodeSchema,
    message: z.string().min(1),
    retryable: z.boolean(),
    action: z.string().min(1).optional(),
    details: optionalJsonValueSchema,
  })
  .strict();
export type AppError = z.infer<typeof appErrorSchema>;
