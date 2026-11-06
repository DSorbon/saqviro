import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import type { JobInput, JobSnapshot } from "./job";
import type { ParamValue, Tool } from "./types";
import type { WorkflowStep } from "./workflow";

export const listTools = () => invoke<Tool[]>("list_tools");

export const toolsForFiles = (paths: string[]) => invoke<string[]>("tools_for_files", { paths });

/** Queues a job and returns its id. Rejects with a `JobError` if the request is invalid. */
export const runTool = (toolId: string, input: JobInput, params: Record<string, ParamValue>, outputDir: string | null = null) =>
  invoke<number>("run_tool", { toolId, input, params, outputDir });

/** Queues a workflow as one job. Rejects with a `JobError` if it cannot run. */
export const runWorkflow = (steps: WorkflowStep[], input: JobInput, outputDir: string | null = null) =>
  invoke<number>("run_workflow", { steps, input, outputDir });

export interface ImagePreview {
  /** A JPEG, in Base64. */
  data: string;
  /** The size of the image itself, not of the preview. */
  width: number;
  height: number;
}

export const imagePreview = (path: string) => invoke<ImagePreview>("image_preview", { path });

/** Native folder picker. Resolves to null if the user cancels. */
export const pickFolder = () => invoke<string | null>("pick_folder");

export const cancelJob = (id: number) => invoke<boolean>("cancel_job", { id });

/** Native file picker, filtered to what the tool accepts (or any tool, without an id). */
export const pickFiles = (toolId?: string) => invoke<string[]>("pick_files", { toolId: toolId ?? null });

/** Only files created by a job can be revealed or opened. */
export const revealFile = (path: string) => invoke<void>("reveal_file", { path });

export const openFile = (path: string) => invoke<void>("open_file", { path });

/** Asks where to save a job's image. Resolves to false if the user cancels. */
export const saveImage = (id: number) => invoke<boolean>("save_image", { id });

export interface AvailableUpdate {
  version: string;
  notes: string | null;
}

/** False in builds that have no key to verify updates with. */
export const updatesEnabled = () => invoke<boolean>("updates_enabled");

export const checkUpdate = () => invoke<AvailableUpdate | null>("check_update");

/** Installs and restarts the app. */
export const installUpdate = () => invoke<void>("install_update");

export const openLicenses = () => invoke<void>("open_licenses");

export const appVersion = () => getVersion();

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
