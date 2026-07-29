import { randomBytes } from "node:crypto";

export const OPENCODE_SERVER_USERNAME = "opencode" as const;

export interface ServerCredentials {
  readonly username: typeof OPENCODE_SERVER_USERNAME;
  password: string;
}

/** Credentials stay in the Main runtime and are never part of a public snapshot. */
export function createServerCredentials(): ServerCredentials {
  return {
    username: OPENCODE_SERVER_USERNAME,
    password: randomBytes(32).toString("base64url"),
  };
}

export function basicAuthHeader(credentials: ServerCredentials): string {
  return `Basic ${Buffer.from(`${credentials.username}:${credentials.password}`, "utf8").toString("base64")}`;
}

export function serverCredentialEnvironment(credentials: ServerCredentials): Record<string, string> {
  return {
    OPENCODE_SERVER_USERNAME: credentials.username,
    OPENCODE_SERVER_PASSWORD: credentials.password,
  };
}

export function clearServerCredentials(credentials: ServerCredentials): void {
  credentials.password = "";
}

export const createCredentials = createServerCredentials;
export const makeBasicAuthHeader = basicAuthHeader;
