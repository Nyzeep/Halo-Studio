import { z } from "zod";

import {
  agentKindSchema,
  runtimeBindingSchema,
  workspaceIdSchema,
} from "./agent.js";
import {
  appErrorSchema,
  jsonValueSchema,
  type AppError,
} from "./error.js";

export type IpcEnvelope<TData> =
  | { readonly ok: true; readonly data: TData }
  | { readonly ok: false; readonly error: AppError };

type IpcEnvelopeSchema<TDataSchema extends z.ZodTypeAny> = z.ZodType<
  IpcEnvelope<z.output<TDataSchema>>,
  z.ZodTypeDef,
  IpcEnvelope<z.input<TDataSchema>>
>;

export function ipcEnvelope<const TDataSchema extends z.ZodTypeAny>(
  dataSchema: TDataSchema,
): IpcEnvelopeSchema<TDataSchema> {
  const schema = z.discriminatedUnion("ok", [
    z.object({ ok: z.literal(true), data: dataSchema }).strict(),
    z.object({ ok: z.literal(false), error: appErrorSchema }).strict(),
  ]);

  return schema as IpcEnvelopeSchema<TDataSchema>;
}

export const ipcEnvelopeSchema = ipcEnvelope(jsonValueSchema);

export const trustStateSchema = z.enum(["untrusted", "trusted"]);
export type TrustState = z.infer<typeof trustStateSchema>;

export const workspaceCandidateSchema = z
  .object({
    selectionId: z.string().uuid(),
    displayPath: z.string().min(1),
  })
  .strict();
export type WorkspaceCandidate = z.infer<typeof workspaceCandidateSchema>;

export const workspaceSchema = z
  .object({
    id: workspaceIdSchema,
    rootPath: z.string().min(1),
    realPath: z.string().min(1),
    trustState: trustStateSchema,
  })
  .strict();
export type Workspace = z.infer<typeof workspaceSchema>;

const unsafeConfigPathKeys = new Set([
  "__proto__",
  "prototype",
  "constructor",
]);

export const configPathSegmentSchema = z.union([
  z
    .string()
    .min(1)
    .max(256)
    .refine((key) => !unsafeConfigPathKeys.has(key), {
      message: "Prototype-sensitive keys are not allowed in config paths.",
    }),
  z.number().int().nonnegative().safe().max(1_000_000),
]);
export type ConfigPathSegment = z.infer<typeof configPathSegmentSchema>;

const configPathSchema = z.array(configPathSegmentSchema).min(1).max(128);

export const configOperationSchema = z.discriminatedUnion("op", [
  z
    .object({
      op: z.literal("set"),
      path: configPathSchema,
      value: jsonValueSchema,
    })
    .strict(),
  z
    .object({
      op: z.literal("remove"),
      path: configPathSchema,
    })
    .strict(),
]);
export type ConfigOperation = z.infer<typeof configOperationSchema>;

const opaqueIdSchema = z.string().min(1);
const fingerprintSchema = z
  .string()
  .regex(/^[0-9a-f]{64}$/u, "Expected a lowercase SHA-256 digest.");

export const configPreviewSchema = z
  .object({
    previewId: opaqueIdSchema,
    targetId: opaqueIdSchema,
    fingerprint: fingerprintSchema,
    unifiedDiff: z.string(),
    restartRequired: z.array(agentKindSchema),
  })
  .strict();
export type ConfigPreview = z.infer<typeof configPreviewSchema>;

export const configCommitResultSchema = z
  .object({
    backupId: opaqueIdSchema,
    targetId: opaqueIdSchema,
    fingerprint: fingerprintSchema,
  })
  .strict();
export type ConfigCommitResult = z.infer<typeof configCommitResultSchema>;

export const configRollbackResultSchema = z
  .object({
    backupId: opaqueIdSchema,
    targetId: opaqueIdSchema,
    fingerprint: fingerprintSchema,
  })
  .strict();
export type ConfigRollbackResult = z.infer<typeof configRollbackResultSchema>;

export const storageHealthSchema = z
  .object({
    mode: z.enum(["read-write", "read-only-recovery"]),
    schemaVersion: z.number().int().nonnegative(),
    diagnostics: z.array(z.string()),
  })
  .strict();
export type StorageHealth = z.infer<typeof storageHealthSchema>;

export type EmptyRequest = Record<string, never>;

const emptyRequestSchema = z.object({}).strict() as z.ZodType<
  EmptyRequest,
  z.ZodTypeDef,
  EmptyRequest
>;
const workspaceIdFilterSchema = z
  .object({ workspaceId: workspaceIdSchema.optional() })
  .strict();
const runtimeActionRequestSchema = z
  .object({ workspaceId: workspaceIdSchema, agentKind: agentKindSchema })
  .strict();

export const ipcContracts = {
  "workspace.pick": {
    request: emptyRequestSchema,
    data: workspaceCandidateSchema.nullable(),
    response: ipcEnvelope(workspaceCandidateSchema.nullable()),
  },
  "workspace.open": {
    request: z.object({ selectionId: z.string().uuid() }).strict(),
    data: workspaceSchema,
    response: ipcEnvelope(workspaceSchema),
  },
  "workspace.snapshot": {
    request: emptyRequestSchema,
    data: z.array(workspaceSchema),
    response: ipcEnvelope(z.array(workspaceSchema)),
  },
  "workspace.trust": {
    request: z
      .object({ workspaceId: workspaceIdSchema, trustState: trustStateSchema })
      .strict(),
    data: workspaceSchema,
    response: ipcEnvelope(workspaceSchema),
  },
  "runtime.probe": {
    request: workspaceIdFilterSchema,
    data: z.array(runtimeBindingSchema),
    response: ipcEnvelope(z.array(runtimeBindingSchema)),
  },
  "runtime.start": {
    request: runtimeActionRequestSchema,
    data: runtimeBindingSchema,
    response: ipcEnvelope(runtimeBindingSchema),
  },
  "runtime.stop": {
    request: runtimeActionRequestSchema,
    data: runtimeBindingSchema,
    response: ipcEnvelope(runtimeBindingSchema),
  },
  "runtime.snapshot": {
    request: workspaceIdFilterSchema,
    data: z.array(runtimeBindingSchema),
    response: ipcEnvelope(z.array(runtimeBindingSchema)),
  },
  "config.preview": {
    request: z
      .object({
        targetId: opaqueIdSchema,
        operations: z.array(configOperationSchema).min(1),
      })
      .strict(),
    data: configPreviewSchema,
    response: ipcEnvelope(configPreviewSchema),
  },
  "config.commit": {
    request: z.object({ previewId: opaqueIdSchema }).strict(),
    data: configCommitResultSchema,
    response: ipcEnvelope(configCommitResultSchema),
  },
  "config.rollback": {
    request: z.object({ backupId: opaqueIdSchema }).strict(),
    data: configRollbackResultSchema,
    response: ipcEnvelope(configRollbackResultSchema),
  },
  "storage.health": {
    request: emptyRequestSchema,
    data: storageHealthSchema,
    response: ipcEnvelope(storageHealthSchema),
  },
} as const;

export type IpcContractMap = typeof ipcContracts;
export type IpcChannel = keyof typeof ipcContracts;
export type InputOf<TChannel extends IpcChannel> = z.input<
  (typeof ipcContracts)[TChannel]["request"]
>;
export type DataOf<TChannel extends IpcChannel> = z.output<
  (typeof ipcContracts)[TChannel]["data"]
>;
export type ResponseOf<TChannel extends IpcChannel> = z.output<
  (typeof ipcContracts)[TChannel]["response"]
>;
