import * as z from "zod";

export interface FieldIssue {
  message: string;
  path: string;
}

export interface ValidationErrors {
  fieldErrors: Record<string, string[]>;
  formErrors: string[];
  issues: FieldIssue[];
}

export type ValidationResult<Data> =
  | {
      data: Data;
      ok: true;
    }
  | {
      errors: ValidationErrors;
      ok: false;
    };

export class SchemaValidationError extends Error {
  readonly errors: ValidationErrors;

  constructor(message: string, errors: ValidationErrors) {
    super(message);
    this.name = "SchemaValidationError";
    this.errors = errors;
  }
}

export function validateWithSchema<Schema extends z.ZodType>(
  schema: Schema,
  input: unknown,
): ValidationResult<z.output<Schema>> {
  const result = schema.safeParse(input);
  if (result.success) {
    return {
      data: result.data,
      ok: true,
    };
  }

  return {
    errors: toValidationErrors(result.error),
    ok: false,
  };
}

export function parseSchemaOrThrow<Schema extends z.ZodType>(
  schema: Schema,
  input: unknown,
  message = "Invalid schema input",
): z.output<Schema> {
  const result = validateWithSchema(schema, input);
  if (result.ok) {
    return result.data;
  }

  throw new SchemaValidationError(message, result.errors);
}

export function parseSchemaOrFallback<Schema extends z.ZodType>(
  schema: Schema,
  input: unknown,
  fallback: z.output<Schema>,
): z.output<Schema> {
  const result = validateWithSchema(schema, input);
  return result.ok ? result.data : fallback;
}

export function toValidationErrors(error: z.ZodError): ValidationErrors {
  const flattened = z.flattenError(error);

  return {
    fieldErrors: flattened.fieldErrors,
    formErrors: flattened.formErrors,
    issues: error.issues.map((issue) => ({
      message: issue.message,
      path: issue.path.map(String).join("."),
    })),
  };
}
