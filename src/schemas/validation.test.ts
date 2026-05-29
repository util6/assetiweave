import { describe, expect, it } from "vitest";
import * as z from "zod";
import {
  SchemaValidationError,
  parseSchemaOrFallback,
  parseSchemaOrThrow,
  validateWithSchema,
} from "./validation";

describe("schema validation helpers", () => {
  const schema = z.strictObject({
    name: z.string().min(1),
    priority: z.number().int().default(0),
  });

  it("returns parsed data with schema defaults", () => {
    const result = validateWithSchema(schema, { name: "Codex" });

    expect(result).toEqual({
      data: {
        name: "Codex",
        priority: 0,
      },
      ok: true,
    });
  });

  it("formats field errors without throwing", () => {
    const result = validateWithSchema(schema, { name: "", priority: 1.5 });

    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.errors.fieldErrors.name).toEqual([expect.any(String)]);
      expect(result.errors.fieldErrors.priority).toEqual([expect.any(String)]);
      expect(result.errors.issues).toEqual(
        expect.arrayContaining([
          expect.objectContaining({ path: "name" }),
          expect.objectContaining({ path: "priority" }),
        ]),
      );
    }
  });

  it("throws a typed validation error when requested", () => {
    expect(() => parseSchemaOrThrow(schema, { name: "" }, "Invalid source")).toThrow(SchemaValidationError);
  });

  it("returns fallback data for invalid persisted values", () => {
    expect(parseSchemaOrFallback(schema, { name: "" }, { name: "Fallback", priority: 1 })).toEqual({
      name: "Fallback",
      priority: 1,
    });
  });
});
