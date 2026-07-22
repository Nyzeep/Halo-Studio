import type {
  AppError,
  DataOf,
  InputOf,
  IpcContractMap,
  ResponseOf,
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
