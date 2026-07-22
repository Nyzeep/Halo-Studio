export {
  TargetRegistry,
  UnsafeConfigError,
  registerDefaultConfigTargets,
} from "./targetRegistry.js";
export type {
  ConfigScope,
  ConfigOwner,
  ConfigFormat,
  ConfigSource,
  ConfigTargetKind,
  TargetRegistration,
  ConfigTarget,
  DefaultConfigTargetPath,
  DefaultConfigTargetPaths,
} from "./targetRegistry.js";
export {
  ConfigTransaction,
  ConfigConflict,
  ConfigPreviewUnavailable,
  ConfigBackupUnavailable,
  ConfigWriteError,
  ConfigRecoveryError,
} from "./configTransaction.js";
export type {
  ConfigRecoveryReason,
  ConfigTransactionOptions,
} from "./configTransaction.js";
export { ConfigParseError, ConfigPatchError } from "./jsoncPatch.js";
