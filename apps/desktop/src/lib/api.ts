import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import type { Tool } from "./types";

export const listTools = () => invoke<Tool[]>("list_tools");

export const toolsForFiles = (paths: string[]) => invoke<string[]>("tools_for_files", { paths });

/** Calls `onDrop` with the dropped file paths; `onHover` while files are over the window. */
export function onFileDrop(onDrop: (paths: string[]) => void, onHover: (active: boolean) => void) {
  return getCurrentWebview().onDragDropEvent(({ payload }) => {
    if (payload.type === "enter" || payload.type === "over") onHover(true);
    else if (payload.type === "leave") onHover(false);
    else if (payload.type === "drop") {
      onHover(false);
      onDrop(payload.paths);
    }
  });
}
