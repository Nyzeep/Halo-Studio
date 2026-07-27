import { CoreError } from "@halo-studio/core";
import { createHash, randomUUID } from "node:crypto";
import { mkdir, open, rename, rm, unlink } from "node:fs/promises";
import { join } from "node:path";

const AUTHENTICATION_ERROR_MESSAGE = "Credential protection failed";
const MAX_REFERENCE_BYTES = 1_024;
const MAX_SECRET_BYTES = 1024 * 1024;
const MAX_CIPHERTEXT_BYTES = MAX_SECRET_BYTES + 64 * 1024;

export interface SecretProtector {
  isAvailable(): boolean;
  protect(value: Buffer): Buffer;
  unprotect(value: Buffer): Buffer;
}

export interface CredentialVault {
  store(reference: string, value: string): Promise<void>;
  get(reference: string): Promise<string | null>;
  delete(reference: string): Promise<void>;
  isAvailable(): boolean;
}

function authenticationFailed(): CoreError {
  return new CoreError("AuthenticationFailed", AUTHENTICATION_ERROR_MESSAGE);
}

function validatedString(value: unknown, maximumBytes: number): string {
  if (typeof value !== "string" || value.length === 0) {
    throw authenticationFailed();
  }
  const byteLength = Buffer.byteLength(value, "utf8");
  if (byteLength === 0 || byteLength > maximumBytes) {
    throw authenticationFailed();
  }
  return value;
}

function referenceFilename(reference: unknown): string {
  const validated = validatedString(reference, MAX_REFERENCE_BYTES);
  return `${createHash("sha256").update(validated, "utf8").digest("hex")}.credential`;
}

function isMissingFile(error: unknown): boolean {
  if (typeof error !== "object" || error === null) {
    return false;
  }
  try {
    const descriptor = Object.getOwnPropertyDescriptor(error, "code");
    return (
      descriptor !== undefined &&
      "value" in descriptor &&
      descriptor.value === "ENOENT"
    );
  } catch {
    return false;
  }
}

function buffersEqual(left: Buffer, right: Buffer): boolean {
  return (
    left.length === right.length && Buffer.prototype.equals.call(left, right)
  );
}

function zeroBuffer(value: Buffer | undefined): void {
  if (value !== undefined) {
    Buffer.prototype.fill.call(value, 0);
  }
}

interface DirectorySyncHandle {
  sync(): Promise<void>;
  close(): Promise<void>;
}

type OpenDirectoryForSync = (path: string) => Promise<DirectorySyncHandle>;

async function openDirectoryForSync(path: string): Promise<DirectorySyncHandle> {
  return open(path, "r");
}

export async function syncParentDirectoryAfterRename(
  directory: string,
  platform: NodeJS.Platform = process.platform,
  openDirectory: OpenDirectoryForSync = openDirectoryForSync,
): Promise<void> {
  if (platform === "win32") {
    return;
  }

  let handle: DirectorySyncHandle | undefined;
  let failed = false;
  try {
    handle = await openDirectory(directory);
    await handle.sync();
  } catch {
    failed = true;
  }
  if (handle !== undefined) {
    try {
      await handle.close();
    } catch {
      failed = true;
    }
  }
  if (failed) {
    throw authenticationFailed();
  }
}

async function readBoundedCiphertext(path: string): Promise<Buffer> {
  const handle = await open(path, "r");
  let scratch: Buffer | undefined;
  try {
    const fileStat = await handle.stat();
    if (
      !Number.isSafeInteger(fileStat.size) ||
      fileStat.size < 1 ||
      fileStat.size > MAX_CIPHERTEXT_BYTES
    ) {
      throw authenticationFailed();
    }

    scratch = Buffer.allocUnsafe(MAX_CIPHERTEXT_BYTES + 1);
    let totalBytesRead = 0;
    while (totalBytesRead < scratch.length) {
      const { bytesRead } = await handle.read(
        scratch,
        totalBytesRead,
        scratch.length - totalBytesRead,
        totalBytesRead,
      );
      if (bytesRead === 0) {
        break;
      }
      totalBytesRead += bytesRead;
    }
    if (totalBytesRead < 1 || totalBytesRead > MAX_CIPHERTEXT_BYTES) {
      throw authenticationFailed();
    }
    return Buffer.from(scratch.subarray(0, totalBytesRead));
  } finally {
    zeroBuffer(scratch);
    try {
      await handle.close();
    } catch {
      throw authenticationFailed();
    }
  }
}

export class FileCredentialVault implements CredentialVault {
  readonly #directory: string;
  readonly #protector: SecretProtector;

  constructor(directory: string, protector: SecretProtector) {
    if (typeof directory !== "string" || directory.length === 0) {
      throw authenticationFailed();
    }
    this.#directory = directory;
    this.#protector = protector;
  }

  isAvailable(): boolean {
    try {
      return this.#protector.isAvailable() === true;
    } catch {
      return false;
    }
  }

  async store(reference: string, value: string): Promise<void> {
    const filename = referenceFilename(reference);
    const validatedValue = validatedString(value, MAX_SECRET_BYTES);
    if (!this.isAvailable()) {
      throw authenticationFailed();
    }

    const plaintext = Buffer.from(validatedValue, "utf8");
    let encrypted: Buffer | undefined;
    let temporaryPath: string | undefined;
    try {
      let unchanged = false;
      let protectedValue: Buffer | undefined;
      try {
        const output = this.#protector.protect(plaintext);
        if (!Buffer.isBuffer(output)) {
          throw authenticationFailed();
        }
        protectedValue = output;
        encrypted = Buffer.from(output);
        unchanged = buffersEqual(plaintext, encrypted);
      } finally {
        zeroBuffer(plaintext);
        if (protectedValue !== plaintext) {
          zeroBuffer(protectedValue);
        }
      }
      if (
        unchanged ||
        encrypted === undefined ||
        encrypted.length < 1 ||
        encrypted.length > MAX_CIPHERTEXT_BYTES
      ) {
        throw authenticationFailed();
      }

      await mkdir(this.#directory, { mode: 0o700, recursive: true });
      const destination = join(this.#directory, filename);
      temporaryPath = join(
        this.#directory,
        `.${filename}.${randomUUID()}.tmp`,
      );
      const handle = await open(temporaryPath, "wx", 0o600);
      try {
        await handle.writeFile(encrypted);
        await handle.sync();
      } finally {
        await handle.close();
      }
      await rename(temporaryPath, destination);
      await syncParentDirectoryAfterRename(this.#directory);
      temporaryPath = undefined;
    } catch {
      if (temporaryPath !== undefined) {
        try {
          await rm(temporaryPath, { force: true });
        } catch {
          // The fixed failure boundary must not expose cleanup details.
        }
      }
      throw authenticationFailed();
    } finally {
      zeroBuffer(plaintext);
      zeroBuffer(encrypted);
    }
  }

  async get(reference: string): Promise<string | null> {
    const filename = referenceFilename(reference);
    if (!this.isAvailable()) {
      throw authenticationFailed();
    }

    let encrypted: Buffer;
    try {
      encrypted = await readBoundedCiphertext(join(this.#directory, filename));
    } catch (error) {
      if (isMissingFile(error)) {
        return null;
      }
      throw authenticationFailed();
    }

    let plaintext: Buffer | undefined;
    let unprotected: Buffer | undefined;
    try {
      const output = this.#protector.unprotect(encrypted);
      if (!Buffer.isBuffer(output)) {
        throw authenticationFailed();
      }
      unprotected = output;
      if (output.length < 1 || output.length > MAX_SECRET_BYTES) {
        throw authenticationFailed();
      }
      plaintext = Buffer.from(output);
      zeroBuffer(unprotected);
      const value = new TextDecoder("utf-8", {
        fatal: true,
        ignoreBOM: true,
      }).decode(plaintext);
      if (Buffer.byteLength(value, "utf8") !== plaintext.length) {
        throw authenticationFailed();
      }
      return value;
    } catch {
      throw authenticationFailed();
    } finally {
      zeroBuffer(unprotected);
      zeroBuffer(encrypted);
      zeroBuffer(plaintext);
    }
  }

  async delete(reference: string): Promise<void> {
    const filename = referenceFilename(reference);
    if (!this.isAvailable()) {
      throw authenticationFailed();
    }
    try {
      await unlink(join(this.#directory, filename));
    } catch (error) {
      if (!isMissingFile(error)) {
        throw authenticationFailed();
      }
    }
  }
}
