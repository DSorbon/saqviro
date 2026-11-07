import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import type { JobInput, JobSnapshot } from "./job";
import type { ParamValue, Tool } from "./types";
import type { WorkflowStep } from "./workflow";

export const listTools = () => invoke<Tool[]>("list_tools");

export const toolsForFiles = (paths: string[]) => invoke<string[]>("tools_for_files", { paths });

export interface Advice {
  tool: string;
  reason: string;
}

/** Smart Drop: what stands out about these files. */
export const adviseFiles = (paths: string[]) => invoke<Advice[]>("advise_files", { paths });

/** Queues a job and returns its id. Rejects with a `JobError` if the request is invalid. */
export const runTool = (toolId: string, input: JobInput, params: Record<string, ParamValue>, outputDir: string | null = null) =>
  invoke<number>("run_tool", { toolId, input, params, outputDir });

/** Queues a workflow as one job. Rejects with a `JobError` if it cannot run. */
export const runWorkflow = (steps: WorkflowStep[], input: JobInput, outputDir: string | null = null, keepGoing = false) =>
  invoke<number>("run_workflow", { steps, input, outputDir, keepGoing });

export interface ImagePreview {
  /** A JPEG, in Base64. */
  data: string;
  /** The size of the image itself, not of the preview. */
  width: number;
  height: number;
}

export const imagePreview = (path: string) => invoke<ImagePreview>("image_preview", { path });

export interface Thumbnails {
  /** JPEGs in Base64, in page order. */
  pages: string[];
  count: number;
}

export const pdfThumbnails = (path: string) => invoke<Thumbnails>("pdf_thumbnails", { path });

export interface ToolPreview {
  /** JPEGs in Base64, when they can be shown. */
  data: string | null;
  before: string | null;
  size: number;
  source_size: number | null;
  /** The size comes from a few seconds of the film. */
  estimated: boolean;
  /** The shape of the sound before and after, on one scale. */
  peaks_before: number[] | null;
  peaks_after: number[] | null;
  /** The start of a result that is text. */
  text: string | null;
  /** Small pictures of a result that is a folder, and how many files it has. */
  items: string[] | null;
  count: number | null;
}

export interface ArchiveContents {
  count: number;
  total: number;
  /** Names and sizes of the first entries. */
  entries: [string, number][];
  /** There is more than was counted. */
  more: boolean;
}

/** What is inside an archive, without unpacking it. */
export const archiveContents = (path: string) => invoke<ArchiveContents>("archive_contents", { path });

/** What a tool would make of one file, before anything is saved. */
export const previewTool = (toolId: string, paths: string[], params: Record<string, ParamValue>) =>
  invoke<ToolPreview>("preview_tool", { toolId, paths, params });

/** What a reporting tool says about a file, without keeping anything. */
export const fileFacts = (toolId: string, path: string) => invoke<string>("file_facts", { toolId, path });

/** The size of each file; folders have none. */
export const fileSizes = (paths: string[]) => invoke<(number | null)[]>("file_sizes", { paths });

/** The new name of each file. Rejects with why the renaming cannot be done. */
export const planRename = (paths: string[], params: Record<string, ParamValue>) =>
  invoke<[string, string][]>("plan_rename", { paths, params });

export interface Timeline {
  duration: number;
  /** JPEGs in Base64, evenly spread over the file. */
  frames: string[];
  /** Loudest moment of each slice, from 0 to 1. */
  peaks: number[];
}

/** Frames and sound of a video or audio file. */
export const mediaTimeline = async (path: string) =>
  JSON.parse(await invoke<string>("media_timeline", { path })) as Timeline;

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
