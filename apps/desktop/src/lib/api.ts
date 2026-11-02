import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import type { JobInput, JobSnapshot } from "./job";
import type { ParamValue, Tool } from "./types";

export const listTools = () => invoke<Tool[]>("list_tools");

export const toolsForFiles = (paths: string[]) => invoke<string[]>("tools_for_files", { paths });

/** Queues a job and returns its id. Rejects with a `JobError` if the request is invalid. */
export const runTool = (toolId: string, input: JobInput, params: Record<string, ParamValue>) =>
  invoke<number>("run_tool", { toolId, input, params });

export const cancelJob = (id: number) => invoke<boolean>("cancel_job", { id });

/** Native file picker, filtered to what the tool accepts (or any tool, without an id). */
export const pickFiles = (toolId?: string) => invoke<string[]>("pick_files", { toolId: toolId ?? null });

/** Only files created by a job can be revealed or opened. */
export const revealFile = (path: string) => invoke<void>("reveal_file", { path });

export const openFile = (path: string) => invoke<void>("open_file", { path });

/** Every change of every job. Returns a function that stops listening. */
export const onJobEvent = (cb: (job: JobSnapshot) => void) => listen<JobSnapshot>("job", (e) => cb(e.payload));

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
