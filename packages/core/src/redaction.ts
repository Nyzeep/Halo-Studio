import type { JsonValue } from "@halo-studio/contracts";
import { types as utilTypes } from "node:util";

export const REDACTED = "[REDACTED]";
export const TRUNCATED = "[TRUNCATED]";
export const UNSERIALIZABLE = "[UNSERIALIZABLE]";

const SENSITIVE_KEY_PATTERN =
  /authorization|api.?key|token|secret|password|cookie/iu;

const DEFAULT_MAX_DEPTH = 8;
const DEFAULT_MAX_NODES = 1_000;
const DEFAULT_MAX_STRING_LENGTH = 2_048;
const DEFAULT_MAX_CONTAINER_ENTRIES = 100;

const HARD_MAX_DEPTH = 32;
const HARD_MAX_NODES = 10_000;
const HARD_MAX_STRING_LENGTH = 65_536;
const HARD_MAX_CONTAINER_ENTRIES = 1_000;

export interface RedactionOptions {
  readonly maxContainerEntries?: number;
  readonly maxDepth?: number;
  readonly maxNodes?: number;
  readonly maxStringLength?: number;
}

interface ResolvedRedactionOptions {
  readonly maxContainerEntries: number;
  readonly maxDepth: number;
  readonly maxNodes: number;
  readonly maxStringLength: number;
}

interface RedactionState {
  readonly activeAncestors: WeakSet<object>;
  readonly options: ResolvedRedactionOptions;
  visitedNodes: number;
}

function boundedOption(
  value: number | undefined,
  fallback: number,
  hardMaximum: number,
): number {
  if (value === undefined || !Number.isSafeInteger(value) || value < 1) {
    return fallback;
  }
  return Math.min(value, hardMaximum);
}

function resolveOptions(options: RedactionOptions): ResolvedRedactionOptions {
  return {
    maxContainerEntries: boundedOption(
      options.maxContainerEntries,
      DEFAULT_MAX_CONTAINER_ENTRIES,
      HARD_MAX_CONTAINER_ENTRIES,
    ),
    maxDepth: boundedOption(
      options.maxDepth,
      DEFAULT_MAX_DEPTH,
      HARD_MAX_DEPTH,
    ),
    maxNodes: boundedOption(
      options.maxNodes,
      DEFAULT_MAX_NODES,
      HARD_MAX_NODES,
    ),
    maxStringLength: boundedOption(
      options.maxStringLength,
      DEFAULT_MAX_STRING_LENGTH,
      HARD_MAX_STRING_LENGTH,
    ),
  };
}

function truncateString(value: string, maximumLength: number): string {
  return value.length <= maximumLength
    ? value
    : `${String.prototype.slice.call(value, 0, maximumLength)}${TRUNCATED}`;
}

function dataProperty(
  value: object,
  key: string,
): unknown | typeof UNSERIALIZABLE {
  let current: object | null = value;
  let remainingPrototypeDepth = 4;

  while (current !== null && remainingPrototypeDepth > 0) {
    if (utilTypes.isProxy(current)) {
      return UNSERIALIZABLE;
    }
    const descriptor = Object.getOwnPropertyDescriptor(current, key);
    if (descriptor !== undefined) {
      return "value" in descriptor ? descriptor.value : UNSERIALIZABLE;
    }
    current = Object.getPrototypeOf(current) as object | null;
    remainingPrototypeDepth -= 1;
  }

  return UNSERIALIZABLE;
}

function redactError(
  error: Error,
  depth: number,
  state: RedactionState,
): JsonValue {
  const rawName = dataProperty(error, "name");
  const rawMessage = dataProperty(error, "message");
  const name = typeof rawName === "string" ? rawName : "Error";
  const message = typeof rawMessage === "string" ? rawMessage : "";

  return {
    name: redactValue(name, depth + 1, state),
    message: redactValue(message, depth + 1, state),
  };
}

function redactArray(
  value: readonly unknown[],
  depth: number,
  state: RedactionState,
): JsonValue {
  const lengthDescriptor = Object.getOwnPropertyDescriptor(value, "length");
  if (
    lengthDescriptor === undefined ||
    !("value" in lengthDescriptor) ||
    typeof lengthDescriptor.value !== "number" ||
    !Number.isSafeInteger(lengthDescriptor.value) ||
    lengthDescriptor.value < 0
  ) {
    return UNSERIALIZABLE;
  }

  const length = lengthDescriptor.value;
  const inspectedLength = Math.min(
    length,
    state.options.maxContainerEntries,
  );
  const output: JsonValue[] = [];

  for (let index = 0; index < inspectedLength; index += 1) {
    const descriptor = Object.getOwnPropertyDescriptor(value, String(index));
    if (descriptor === undefined) {
      output.push(null);
    } else if ("value" in descriptor) {
      output.push(redactValue(descriptor.value, depth + 1, state));
    } else {
      output.push(UNSERIALIZABLE);
    }
  }

  if (length > inspectedLength) {
    output.push(TRUNCATED);
  }
  return output;
}

function defineJsonProperty(
  target: Record<string, JsonValue>,
  key: string,
  value: JsonValue,
): void {
  Object.defineProperty(target, key, {
    configurable: true,
    enumerable: true,
    value,
    writable: true,
  });
}

function redactObject(
  value: object,
  depth: number,
  state: RedactionState,
): JsonValue {
  const prototype = Object.getPrototypeOf(value) as object | null;
  if (prototype !== null && prototype !== Object.prototype) {
    return UNSERIALIZABLE;
  }

  const output: Record<string, JsonValue> = {};
  let inspectedEntries = 0;
  let truncated = false;

  for (const key in value) {
    const descriptor = Object.getOwnPropertyDescriptor(value, key);
    if (descriptor === undefined || !descriptor.enumerable) {
      continue;
    }

    if (inspectedEntries >= state.options.maxContainerEntries) {
      truncated = true;
      break;
    }
    inspectedEntries += 1;
    const outputKey = truncateString(key, state.options.maxStringLength);

    if (SENSITIVE_KEY_PATTERN.test(key)) {
      defineJsonProperty(output, outputKey, REDACTED);
    } else if ("value" in descriptor) {
      defineJsonProperty(
        output,
        outputKey,
        redactValue(descriptor.value, depth + 1, state),
      );
    } else {
      defineJsonProperty(output, outputKey, UNSERIALIZABLE);
    }
  }

  if (truncated) {
    defineJsonProperty(output, TRUNCATED, TRUNCATED);
  }
  return output;
}

function redactValue(
  value: unknown,
  depth: number,
  state: RedactionState,
): JsonValue {
  state.visitedNodes += 1;
  if (
    state.visitedNodes > state.options.maxNodes ||
    depth > state.options.maxDepth
  ) {
    return TRUNCATED;
  }

  if (value === null || typeof value === "boolean") {
    return value;
  }
  if (typeof value === "string") {
    return truncateString(value, state.options.maxStringLength);
  }
  if (typeof value === "number") {
    return Number.isFinite(value) ? value : UNSERIALIZABLE;
  }
  if (typeof value === "bigint") {
    return truncateString(`${value}n`, state.options.maxStringLength);
  }
  if (typeof value !== "object") {
    return UNSERIALIZABLE;
  }

  try {
    if (utilTypes.isProxy(value)) {
      return UNSERIALIZABLE;
    }
    if (state.activeAncestors.has(value)) {
      return UNSERIALIZABLE;
    }
    if (utilTypes.isDate(value)) {
      return Number.isFinite(Date.prototype.getTime.call(value))
        ? truncateString(
            Date.prototype.toISOString.call(value),
            state.options.maxStringLength,
          )
        : UNSERIALIZABLE;
    }
    if (utilTypes.isNativeError(value)) {
      return redactError(value, depth, state);
    }

    state.activeAncestors.add(value);
    try {
      return Array.isArray(value)
        ? redactArray(value, depth, state)
        : redactObject(value, depth, state);
    } finally {
      state.activeAncestors.delete(value);
    }
  } catch {
    return UNSERIALIZABLE;
  }
}

export function redactLogValue(
  value: unknown,
  options: RedactionOptions = {},
): JsonValue {
  try {
    const state: RedactionState = {
      activeAncestors: new WeakSet(),
      options: resolveOptions(options),
      visitedNodes: 0,
    };
    return redactValue(value, 0, state);
  } catch {
    return UNSERIALIZABLE;
  }
}
