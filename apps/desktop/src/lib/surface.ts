import { inputKind } from "./job";
import type { Tool } from "./types";

/** Tools that need no files show their result as it is typed, with no Run. */
export const isLive = (tool: Tool) => inputKind(tool) !== "files";
