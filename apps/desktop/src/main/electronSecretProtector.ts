import { CoreError } from "@halo-studio/core";
import type { SecretProtector } from "@halo-studio/storage";

const PREFIX = "halo-secret-v1:";
const ERROR_MESSAGE = "Credential protection is unavailable.";

export interface SafeStoragePort {
  isEncryptionAvailable(): boolean;
  encryptString(value: string): Buffer;
  decryptString(value: Buffer): string;
}

function unavailable(): CoreError {
  return new CoreError("AuthenticationFailed", ERROR_MESSAGE);
}

function isAvailable(port: SafeStoragePort): boolean {
  try {
    return port.isEncryptionAvailable() === true;
  } catch {
    return false;
  }
}

function decodeProtectedString(value: string): Buffer {
  if (!value.startsWith(PREFIX)) throw unavailable();
  const encoded = value.slice(PREFIX.length);
  if (encoded.length === 0 || encoded.length % 4 !== 0 || !/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/u.test(encoded)) {
    throw unavailable();
  }
  const decoded = Buffer.from(encoded, "base64");
  if (decoded.length === 0 || decoded.toString("base64") !== encoded) throw unavailable();
  return decoded;
}

export function createElectronSecretProtector(port: SafeStoragePort): SecretProtector {
  return {
    isAvailable: () => isAvailable(port),
    protect(value) {
      if (!Buffer.isBuffer(value) || value.length === 0 || !isAvailable(port)) throw unavailable();
      try {
        const encrypted = port.encryptString(`${PREFIX}${value.toString("base64")}`);
        if (!Buffer.isBuffer(encrypted) || encrypted.length === 0) throw unavailable();
        return Buffer.from(encrypted);
      } catch {
        throw unavailable();
      }
    },
    unprotect(value) {
      if (!Buffer.isBuffer(value) || value.length === 0 || !isAvailable(port)) throw unavailable();
      try {
        const decrypted = port.decryptString(Buffer.from(value));
        if (typeof decrypted !== "string") throw unavailable();
        return decodeProtectedString(decrypted);
      } catch {
        throw unavailable();
      }
    },
  };
}
