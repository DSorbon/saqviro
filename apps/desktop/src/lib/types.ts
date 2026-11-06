// Mirrors `saqviro_core::manifest`. The Rust types are the source of truth.

export type Category =
  | "pdf" | "image" | "video" | "audio" | "files" | "text" | "qr"
  | "developer" | "privacy" | "network" | "system" | "screen" | "ocr";

export type ParamValue = string | number | boolean;

export interface Parameter {
  id: string;
  type: "integer" | "number" | "boolean" | "string" | "enum";
  optional?: boolean;
  secret?: boolean;
  multiline?: boolean;
  min?: number;
  max?: number;
  values?: string[];
  default?: ParamValue;
}

export interface Preset {
  id: string;
  name: string;
  params?: Record<string, ParamValue>;
}

export interface Tool {
  id: string;
  name: string;
  category: Category;
  description: string;
  keywords: string[];
  inputs: { type: string; multiple?: boolean; optional?: boolean; extensions?: string[] }[];
  outputs?: { type: string; multiple?: boolean }[];
  parameters?: Parameter[];
  presets: Preset[];
  capabilities?: { batch?: boolean; destructive?: boolean };
  ui?: { advanced?: string[]; one_of?: string[]; preview?: string };
}
