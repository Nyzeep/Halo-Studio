const MAX_SESSION_ID_LENGTH = 512;
export const MAX_SESSION_TEXT_LENGTH = 32_768;

export interface ProjectedSession {
  readonly sessionId: string;
  readonly title?: string;
  readonly updatedAt?: string;
}

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function boundedText(value: unknown): string | undefined {
  if (typeof value !== "string" || value.length === 0 || value.length > MAX_SESSION_TEXT_LENGTH) return undefined;
  return value;
}

export function safeSessionId(value: unknown): string | undefined {
  if (typeof value !== "string" || value.length === 0 || value.length > MAX_SESSION_ID_LENGTH) return undefined;
  if (/[\u0000-\u001f\u007f]/u.test(value)) return undefined;
  return value;
}

export function optionalText(record: Record<string, unknown>, key: string): string | undefined | null {
  if (!(key in record) || record[key] === undefined || record[key] === null) return undefined;
  return boundedText(record[key]) ?? null;
}

function optionalUpdatedAt(record: Record<string, unknown>): string | undefined | null {
  if (!("time" in record) || record.time === undefined || record.time === null) return undefined;
  if (!isRecord(record.time) || typeof record.time.updated !== "number" || !Number.isFinite(record.time.updated)) return null;
  const date = new Date(record.time.updated);
  if (Number.isNaN(date.getTime())) return null;
  return date.toISOString();
}

/** Shared safe projection for HTTP responses and SSE session payloads. */
export function projectSession(value: unknown): ProjectedSession | undefined {
  if (!isRecord(value)) return undefined;
  const sessionId = safeSessionId(value.id);
  if (!sessionId) return undefined;
  const title = optionalText(value, "title");
  const updatedAt = optionalUpdatedAt(value);
  if (title === null || updatedAt === null) return undefined;
  return {
    sessionId,
    ...(title === undefined ? {} : { title }),
    ...(updatedAt === undefined ? {} : { updatedAt }),
  };
}
