import { z } from "zod";

export type JsonPrimitive = boolean | null | number | string;
export type JsonValue =
  | JsonPrimitive
  | readonly JsonValue[]
  | { readonly [key: string]: JsonValue };

export const jsonValueSchema: z.ZodType<JsonValue> = z.lazy(() =>
  z.union([
    z.string(),
    z.number().finite(),
    z.boolean(),
    z.null(),
    z.array(jsonValueSchema),
    z.record(jsonValueSchema),
  ]),
);

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
    details: jsonValueSchema.optional(),
  })
  .strict();
export type AppError = z.infer<typeof appErrorSchema>;
