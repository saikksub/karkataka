import type { PropSchema } from "@karkataka/runtime";

/** How the props panel edits one prop. */
export type Kind = "text" | "multiline" | "number" | "boolean" | "select" | "list" | "json";

/** The editor for a prop, from its schema, or from its value when the scene gave no schema. */
export function kindOf(schema: PropSchema | undefined, value: unknown): Kind {
  if (schema?.enum) return "select";
  switch (schema?.type ?? typeOf(value)) {
    case "string":
      return schema?.multiline || (typeof value === "string" && value.includes("\n")) ? "multiline" : "text";
    case "number":
    case "integer":
      return "number";
    case "boolean":
      return "boolean";
    case "array": {
      const strings = Array.isArray(value) && value.every((v) => typeof v === "string");
      return (schema?.items?.type ?? (strings ? "string" : undefined)) === "string" ? "list" : "json";
    }
    default:
      return "json";
  }
}

function typeOf(value: unknown): string {
  return Array.isArray(value) ? "array" : typeof value;
}

/** A number typed into the panel, kept to the schema's `minimum`, `maximum` and `integer`. */
export function toNumber(schema: PropSchema | undefined, text: string): number | undefined {
  let n = Number(text);
  if (text.trim() === "" || !Number.isFinite(n)) return undefined;
  if (schema?.type === "integer") n = Math.round(n);
  if (schema?.minimum !== undefined) n = Math.max(schema.minimum, n);
  if (schema?.maximum !== undefined) n = Math.min(schema.maximum, n);
  return n;
}

/** Props in panel order: those in the schema first, in its order, then the rest of the defaults. */
export function propNames(defaults: object, schema: Record<string, PropSchema> = {}): string[] {
  return [...new Set([...Object.keys(schema), ...Object.keys(defaults)])];
}
