import { z } from "zod";

export type JsonPrimitive = boolean | null | number | string;
export type JsonValue =
  | JsonPrimitive
  | readonly JsonValue[]
  | { readonly [key: string]: JsonValue };

const MAX_JSON_DEPTH = 64;
const MAX_JSON_NODES = 10_000;
const MAX_CONTAINER_ENTRIES = 10_000;
const JSON_VALUE_ERROR_MESSAGE = "Expected a bounded, acyclic JSON value.";

type VisitFrame = {
  readonly kind: "visit";
  readonly value: unknown;
  readonly depth: number;
};

type LeaveFrame = {
  readonly kind: "leave";
  readonly value: object;
};

type ValidationFrame = LeaveFrame | VisitFrame;

function arrayChildren(value: object): unknown[] | undefined {
  const lengthDescriptor = Object.getOwnPropertyDescriptor(value, "length");
  if (
    lengthDescriptor === undefined ||
    typeof lengthDescriptor.value !== "number" ||
    !Number.isSafeInteger(lengthDescriptor.value) ||
    lengthDescriptor.value < 0 ||
    lengthDescriptor.value > MAX_CONTAINER_ENTRIES
  ) {
    return undefined;
  }

  const length = lengthDescriptor.value;
  const keys = Reflect.ownKeys(value);
  if (keys.length !== length + 1 || keys.some((key) => typeof key === "symbol")) {
    return undefined;
  }

  const children = new Array<unknown>(length);
  for (let index = 0; index < length; index += 1) {
    const descriptor = Object.getOwnPropertyDescriptor(value, String(index));
    if (
      descriptor === undefined ||
      !descriptor.enumerable ||
      !("value" in descriptor)
    ) {
      return undefined;
    }
    children[index] = descriptor.value;
  }

  return children;
}

function objectChildren(value: object): unknown[] | undefined {
  const prototype = Object.getPrototypeOf(value);
  if (prototype !== null && prototype !== Object.prototype) {
    return undefined;
  }

  const keys = Reflect.ownKeys(value);
  if (
    keys.length > MAX_CONTAINER_ENTRIES ||
    keys.some((key) => typeof key === "symbol")
  ) {
    return undefined;
  }

  const children: unknown[] = [];
  for (const key of keys) {
    const descriptor = Object.getOwnPropertyDescriptor(value, key);
    if (
      descriptor === undefined ||
      !descriptor.enumerable ||
      !("value" in descriptor)
    ) {
      return undefined;
    }
    children.push(descriptor.value);
  }

  return children;
}

function isJsonValue(input: unknown): input is JsonValue {
  try {
    const activeAncestors = new WeakSet<object>();
    const frames: ValidationFrame[] = [
      { kind: "visit", value: input, depth: 0 },
    ];
    let visitedNodes = 0;

    while (frames.length > 0) {
      const frame = frames.pop();
      if (frame === undefined) {
        return false;
      }

      if (frame.kind === "leave") {
        activeAncestors.delete(frame.value);
        continue;
      }

      visitedNodes += 1;
      if (visitedNodes > MAX_JSON_NODES || frame.depth > MAX_JSON_DEPTH) {
        return false;
      }

      const value = frame.value;
      if (
        value === null ||
        typeof value === "string" ||
        typeof value === "boolean"
      ) {
        continue;
      }
      if (typeof value === "number") {
        if (!Number.isFinite(value)) {
          return false;
        }
        continue;
      }
      if (typeof value !== "object") {
        return false;
      }

      if (activeAncestors.has(value)) {
        return false;
      }

      const children = Array.isArray(value)
        ? arrayChildren(value)
        : objectChildren(value);
      if (children === undefined) {
        return false;
      }

      activeAncestors.add(value);
      frames.push({ kind: "leave", value });
      for (let index = children.length - 1; index >= 0; index -= 1) {
        frames.push({
          kind: "visit",
          value: children[index],
          depth: frame.depth + 1,
        });
      }
    }

    return true;
  } catch {
    return false;
  }
}

type JsonSchemaValue = JsonValue | undefined;
type JsonSchemaPredicate<TValue extends JsonSchemaValue> = (
  input: unknown,
) => input is TValue;

function invalidJsonResult<TValue>(
  params?: Partial<z.ParseParams>,
): z.SafeParseReturnType<unknown, TValue> {
  return {
    success: false,
    error: new z.ZodError<unknown>([
      {
        code: z.ZodIssueCode.custom,
        message: JSON_VALUE_ERROR_MESSAGE,
        path: params?.path ?? [],
      },
    ]),
  };
}

class JsonValueSchema<
  TValue extends JsonSchemaValue,
> extends z.ZodType<TValue, z.ZodTypeDef, unknown> {
  constructor(private readonly predicate: JsonSchemaPredicate<TValue>) {
    super({});
  }

  _parse(input: z.ParseInput): z.ParseReturnType<TValue> {
    if (this.predicate(input.data)) {
      return z.OK(input.data);
    }

    const context: z.ParseContext = {
      common: input.parent.common,
      data: input.data,
      parsedType: z.ZodParsedType.unknown,
      path: input.path,
      parent: input.parent,
      ...(this._def.errorMap === undefined
        ? {}
        : { schemaErrorMap: this._def.errorMap }),
    };
    z.addIssueToContext(context, {
      code: z.ZodIssueCode.custom,
      message: JSON_VALUE_ERROR_MESSAGE,
    });
    return z.INVALID;
  }

  override safeParse(
    data: unknown,
    params?: Partial<z.ParseParams>,
  ): z.SafeParseReturnType<unknown, TValue> {
    try {
      return super.safeParse(data, params);
    } catch {
      return invalidJsonResult<TValue>(params);
    }
  }

  override async safeParseAsync(
    data: unknown,
    params?: Partial<z.ParseParams>,
  ): Promise<z.SafeParseReturnType<unknown, TValue>> {
    try {
      return await super.safeParseAsync(data, params);
    } catch {
      return invalidJsonResult<TValue>(params);
    }
  }
}

export const jsonValueSchema: z.ZodType<
  JsonValue,
  z.ZodTypeDef,
  unknown
> = new JsonValueSchema(isJsonValue);

export const optionalJsonValueSchema: z.ZodType<
  JsonValue | undefined,
  z.ZodTypeDef,
  unknown
> = new JsonValueSchema(
  (input): input is JsonValue | undefined =>
    input === undefined || isJsonValue(input),
);
