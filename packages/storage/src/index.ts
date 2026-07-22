export {
  openDatabase,
  type DatabaseDiagnostics,
  type DatabaseHealth,
  type HaloDatabase,
} from "./database.js";
export {
  FileCredentialVault,
  type CredentialVault,
  type SecretProtector,
} from "./credentialVault.js";
export type {
  CredentialReference,
  CredentialReferenceRepository,
  SaveCredentialReference,
  StorageProvider,
} from "./repositories.js";
