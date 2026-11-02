import type { Tool } from "./types";

export function tool(id: string, name: string, extra: Partial<Tool> = {}): Tool {
  return {
    id,
    name,
    category: id.split(".")[0] as Tool["category"],
    description: "",
    keywords: [],
    inputs: [],
    presets: [{ id: "default", name: "Default" }],
    ...extra,
  };
}
