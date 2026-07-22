import { z } from "zod";

import {
  appErrorSchema,
  jsonValueSchema,
  openCodeEventPayloadSchema,
  piEventPayloadSchema,
  type AppError,
  type DataOf,
  type InputOf,
  type IpcContractMap,
  type ResponseOf,
} from "@halo-studio/contracts";

type IsAny<T> = 0 extends 1 & T ? true : false;
type Assert<T extends true> = T;
type IsNotAny<T> = IsAny<T> extends false ? true : false;

type EmptyInputIsTyped = Assert<IsNotAny<InputOf<"workspace.pick">>>;
type OpenInputIsTyped = Assert<IsNotAny<InputOf<"workspace.open">>>;
type StorageDataIsTyped = Assert<IsNotAny<DataOf<"storage.health">>>;
type StorageResponseIsTyped = Assert<IsNotAny<ResponseOf<"storage.health">>>;
type ContractMapIsTyped = Assert<IsNotAny<IpcContractMap>>;

const emptyInput: InputOf<"workspace.pick"> = {};

// @ts-expect-error Empty requests must reject arbitrary renderer properties.
const invalidEmptyInput: InputOf<"workspace.pick"> = { arbitrary: true };

const openInput: InputOf<"workspace.open"> = {
  selectionId: "13ebf428-5647-4a32-ae2e-55304b4e3e9f",
};

// @ts-expect-error workspace.open requires a selection handle.
const invalidOpenInput: InputOf<"workspace.open"> = {};

const validSetInput: InputOf<"config.preview"> = {
  targetId: "pi:user-settings",
  operations: [{ op: "set", path: ["model"], value: { id: 1 } }],
};

const validRemoveInput: InputOf<"config.preview"> = {
  targetId: "pi:user-settings",
  operations: [{ op: "remove", path: ["model"] }],
};

const missingSetValue: InputOf<"config.preview"> = {
  targetId: "pi:user-settings",
  operations: [
    // @ts-expect-error Config set operations require a value.
    { op: "set", path: ["model"] },
  ],
};

const functionSetValue: InputOf<"config.preview"> = {
  targetId: "pi:user-settings",
  operations: [
    // @ts-expect-error Config values must be JSON-safe.
    { op: "set", path: ["model"], value: () => "invalid" },
  ],
};

const dateSetValue: InputOf<"config.preview"> = {
  targetId: "pi:user-settings",
  operations: [
    // @ts-expect-error Config values must be JSON-safe.
    { op: "set", path: ["model"], value: new Date() },
  ],
};

const undefinedSetValue: InputOf<"config.preview"> = {
  targetId: "pi:user-settings",
  operations: [
    // @ts-expect-error Config set operations cannot use undefined as a value.
    { op: "set", path: ["model"], value: undefined },
  ],
};

// @ts-expect-error Public JSON schema input must be JSON-safe.
const invalidJsonSchemaInput: z.input<typeof jsonValueSchema> = () => "invalid";

type AppErrorInput = z.input<typeof appErrorSchema>;
const appErrorWithoutDetails: AppErrorInput = {
  code: "ProtocolViolation",
  message: "Invalid protocol data",
  retryable: false,
};
const appErrorWithUndefinedDetails: AppErrorInput = {
  ...appErrorWithoutDetails,
  details: undefined,
};

const piEventWithoutData: z.input<typeof piEventPayloadSchema> = {
  protocol: "pi-rpc",
  type: "agent_start",
};
const piEventWithUndefinedData: z.input<typeof piEventPayloadSchema> = {
  ...piEventWithoutData,
  data: undefined,
};
const openCodeEventWithoutData: z.input<typeof openCodeEventPayloadSchema> = {
  protocol: "opencode-sse",
  type: "message.part.updated",
};
const openCodeEventWithUndefinedData: z.input<
  typeof openCodeEventPayloadSchema
> = {
  ...openCodeEventWithoutData,
  data: undefined,
};

const storageData: DataOf<"storage.health"> = {
  mode: "read-write",
  schemaVersion: 1,
  diagnostics: [],
};

const successResponse: ResponseOf<"storage.health"> = {
  ok: true,
  data: storageData,
};

const appError: AppError = {
  code: "MigrationFailed",
  message: "Migration failed",
  retryable: true,
};

const errorResponse: ResponseOf<"storage.health"> = {
  ok: false,
  error: appError,
};

type PublicChannels = keyof IpcContractMap;
const publicChannel: PublicChannels = "storage.health";

void [
  emptyInput,
  invalidEmptyInput,
  openInput,
  invalidOpenInput,
  validSetInput,
  validRemoveInput,
  missingSetValue,
  functionSetValue,
  dateSetValue,
  undefinedSetValue,
  invalidJsonSchemaInput,
  appErrorWithoutDetails,
  appErrorWithUndefinedDetails,
  piEventWithoutData,
  piEventWithUndefinedData,
  openCodeEventWithoutData,
  openCodeEventWithUndefinedData,
  successResponse,
  errorResponse,
  publicChannel,
];

export type PublicContractAssertions =
  | EmptyInputIsTyped
  | OpenInputIsTyped
  | StorageDataIsTyped
  | StorageResponseIsTyped
  | ContractMapIsTyped;
