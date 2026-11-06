// @vitest-environment jsdom
import { fireEvent, render, screen } from "@testing-library/svelte";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { JobSnapshot } from "../lib/job";
import { tool } from "../lib/fixtures";
import ToolView from "./ToolView.svelte";

const api = vi.hoisted(() => ({
  listeners: [] as ((s: JobSnapshot) => void)[],
  runTool: vi.fn(),
  cancelJob: vi.fn(),
  pickFiles: vi.fn(),
  pickFolder: vi.fn(),
  revealFile: vi.fn(),
  openFile: vi.fn(),
  saveImage: vi.fn(),
  imagePreview: vi.fn(),
  pdfThumbnails: vi.fn(),
  previewTool: vi.fn(),
  mediaTimeline: vi.fn(),
}));

vi.mock("../lib/api", () => ({
  runTool: api.runTool,
  cancelJob: api.cancelJob,
  pickFiles: api.pickFiles,
  pickFolder: api.pickFolder,
  revealFile: api.revealFile,
  openFile: api.openFile,
  saveImage: api.saveImage,
  imagePreview: api.imagePreview,
  pdfThumbnails: api.pdfThumbnails,
  previewTool: api.previewTool,
  mediaTimeline: api.mediaTimeline,
  onJobEvent: (cb: (s: JobSnapshot) => void) => {
    api.listeners.push(cb);
    return Promise.resolve(() => {});
  },
}));

const json = tool("developer.json.format", "JSON Formatter", {
  description: "Pretty-print JSON.",
  inputs: [{ type: "text" }],
  parameters: [
    { id: "indent", type: "enum", values: ["2", "minify"], default: "2" },
    { id: "sort_keys", type: "boolean", default: false },
  ],
  presets: [
    { id: "pretty", name: "Pretty", params: { indent: "2" } },
    { id: "minify", name: "Minify", params: { indent: "minify" } },
  ],
  ui: { advanced: ["sort_keys"] },
});

const onFile = { ...json, id: "files.format", inputs: [{ type: "any", extensions: ["*"] }] };

const emit = (over: Partial<JobSnapshot>) =>
  api.listeners.forEach((l) =>
    l({ id: 7, tool_id: json.id, status: "running", progress: 0, output: null, error: null, started_at: 0, finished_at: null, ...over }),
  );

beforeEach(() => {
  api.listeners.length = 0;
  api.runTool.mockReset().mockResolvedValue(7);
  api.cancelJob.mockReset().mockResolvedValue(true);
  api.pickFiles.mockReset().mockResolvedValue([]);
  api.revealFile.mockReset().mockResolvedValue(undefined);
  api.openFile.mockReset().mockResolvedValue(undefined);
  api.saveImage.mockReset().mockResolvedValue(true);
  api.pdfThumbnails.mockReset();
  api.imagePreview.mockReset().mockResolvedValue({ data: "", width: 1, height: 1 });
});

const compress = tool("image.compress", "Compress Images", {
  inputs: [{ type: "image", extensions: ["jpg"] }],
  parameters: [{ id: "quality", type: "integer", min: 1, max: 100, default: 75 }],
  presets: [{ id: "balanced", name: "Balanced", params: { quality: 75 } }],
  ui: { advanced: ["quality"] },
});

describe("ToolView with files", () => {
  it("lists dropped files and runs on them", async () => {
    render(ToolView, { tool: compress, files: ["/photos/a.jpg", "/photos/b.jpg"] });
    expect(screen.getByText("a.jpg")).toBeTruthy();
    expect(screen.getByText("b.jpg")).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: /Run/ }));
    expect(api.runTool).toHaveBeenCalledWith(compress.id, { kind: "files", value: ["/photos/a.jpg", "/photos/b.jpg"] }, { quality: 75 }, null);
  });

  it("adds files from the picker without duplicates and can clear them", async () => {
    api.pickFiles.mockResolvedValue(["/photos/a.jpg", "/photos/c.jpg"]);
    render(ToolView, { tool: compress, files: ["/photos/a.jpg"] });
    await fireEvent.click(screen.getByRole("button", { name: "Choose files…" }));
    expect(api.pickFiles).toHaveBeenCalledWith(compress.id);
    expect(await screen.findByText("c.jpg")).toBeTruthy();
    expect(screen.getAllByText("a.jpg")).toHaveLength(1);

    await fireEvent.click(screen.getByRole("button", { name: "Clear" }));
    expect(screen.queryByText("a.jpg")).toBeNull();
    expect(screen.getByRole("button", { name: /Run/ }).hasAttribute("disabled")).toBe(true);
  });

  it("shows results with sizes and opens or reveals them", async () => {
    render(ToolView, { tool: compress, files: ["/photos/a.jpg"] });
    await fireEvent.click(screen.getByRole("button", { name: /Run/ }));
    const out = { path: "/photos/a-compressed.jpg", size: 348_160, source_size: 1_258_291 };
    emit({ status: "completed", progress: 1, finished_at: 900, output: { kind: "files", value: [out] } });

    expect(await screen.findByText("a-compressed.jpg")).toBeTruthy();
    expect(screen.getByText("1.2 MB → 340 KB (−72%)")).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Show in folder" }));
    expect(api.revealFile).toHaveBeenCalledWith(out.path);
    await fireEvent.click(screen.getByRole("button", { name: "Open" }));
    expect(api.openFile).toHaveBeenCalledWith(out.path);
  });

  it("shows what a result needs to explain", async () => {
    render(ToolView, { tool: compress, files: ["/clips/a.mp4"] });
    await fireEvent.click(screen.getByRole("button", { name: /Run/ }));
    const out = { path: "/clips/a.webm", size: 10, source_size: 20, note: "Saved as WebM." };
    emit({ status: "completed", progress: 1, finished_at: 900, output: { kind: "files", value: [out] } });

    expect(await screen.findByText("Saved as WebM.")).toBeTruthy();
  });
});

describe("ToolView", () => {
  it("shows an image result and saves it", async () => {
    render(ToolView, { tool: onFile, files: ["/a.json"] });
    await fireEvent.click(screen.getByRole("button", { name: /Run/ }));
    emit({ status: "completed", progress: 1, finished_at: 900, output: { kind: "image", value: { ext: "png", data: "QQ==" } } });

    const picture = (await screen.findByAltText("Result")) as HTMLImageElement;
    expect(picture.src).toBe("data:image/png;base64,QQ==");
    await fireEvent.click(screen.getByRole("button", { name: "Save…" }));
    expect(api.saveImage).toHaveBeenCalledOnce();
  });

  it("orders files only where the order matters", async () => {
    const merge = { ...compress, capabilities: { batch: false } };
    const { unmount } = render(ToolView, { tool: merge, files: ["/a.jpg", "/b.jpg"] });
    await screen.findByText("a.jpg");
    await fireEvent.click(screen.getByRole("button", { name: "Move a.jpg down" }));
    expect(screen.getAllByRole("listitem").map((l) => l.textContent?.slice(0, 5))).toEqual(["b.jpg", "a.jpg"]);
    await fireEvent.click(screen.getByRole("button", { name: /Run/ }));
    expect(api.runTool).toHaveBeenCalledWith(merge.id, { kind: "files", value: ["/b.jpg", "/a.jpg"] }, expect.anything(), null);
    unmount();

    render(ToolView, { tool: compress, files: ["/a.jpg", "/b.jpg"] });
    await screen.findByText("a.jpg");
    expect(screen.queryByRole("button", { name: "Move a.jpg down" })).toBeNull();
  });

  it("removes one file from the list", async () => {
    render(ToolView, { tool: compress, files: ["/a.jpg", "/b.jpg"] });
    await fireEvent.click(await screen.findByRole("button", { name: "Remove a.jpg" }));
    expect(screen.queryByText("a.jpg")).toBeNull();
    expect(screen.getByText("b.jpg")).toBeTruthy();
  });

  it("says when text was copied", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, { clipboard: { writeText } });
    render(ToolView, { tool: onFile, files: ["/a.json"] });
    await fireEvent.click(screen.getByRole("button", { name: /Run/ }));
    emit({ status: "completed", progress: 1, finished_at: 900, output: { kind: "text", value: "done" } });
    await fireEvent.click(await screen.findByRole("button", { name: "Copy" }));
    expect(writeText).toHaveBeenCalledWith("done");
    expect(await screen.findByRole("button", { name: "Copied" })).toBeTruthy();
  });

  it("hides secret parameters", () => {
    const tool = { ...json, parameters: [{ id: "password", type: "string" as const, secret: true }], ui: {} };
    const { container } = render(ToolView, { tool, files: [] });
    expect(container.querySelector("input[type=password]")).toBeTruthy();
  });

  it("renders presets and hides advanced parameters", () => {
    render(ToolView, { tool: onFile, files: [] });
    expect(screen.getByRole("radio", { name: "Pretty" }).getAttribute("aria-checked")).toBe("true");
    expect(screen.getByText("Advanced")).toBeTruthy();
    expect(screen.getByRole("button", { name: /Run/ }).hasAttribute("disabled")).toBe(true);
  });

  it("runs with the chosen preset and shows the result", async () => {
    render(ToolView, { tool: onFile, files: ["/a.json"] });
    await fireEvent.click(screen.getByRole("radio", { name: "Minify" }));
    await fireEvent.click(screen.getByRole("button", { name: /Run/ }));

    expect(api.runTool).toHaveBeenCalledWith(onFile.id, { kind: "files", value: ["/a.json"] }, { indent: "minify", sort_keys: false }, null);

    emit({ status: "running", progress: 0.5 });
    expect(await screen.findByText("Working… 50%")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Cancel" })).toBeTruthy();

    emit({ status: "completed", progress: 1, finished_at: 200, output: { kind: "text", value: '{"a":1}' } });
    expect(await screen.findByText("Done in 200 ms")).toBeTruthy();
    expect((screen.getByLabelText("Result") as HTMLTextAreaElement).value).toBe('{"a":1}');
  });

  it("runs with Cmd/Ctrl+Enter, but only when the input is ready", async () => {
    const view = render(ToolView, { tool: onFile, files: [] });
    await fireEvent.keyDown(window, { key: "Enter", metaKey: true });
    expect(api.runTool).not.toHaveBeenCalled();

    await view.rerender({ tool: onFile, files: ["/a.json"] });
    await fireEvent.keyDown(window, { key: "Enter", ctrlKey: true });
    await vi.waitFor(() => expect(api.runTool).toHaveBeenCalledTimes(1));
  });

  it("cancels a running job", async () => {
    render(ToolView, { tool: onFile, files: ["/a.json"] });
    await fireEvent.click(screen.getByRole("button", { name: /Run/ }));
    emit({ status: "running" });
    await fireEvent.click(await screen.findByRole("button", { name: "Cancel" }));
    expect(api.cancelJob).toHaveBeenCalledWith(7);
  });

  it("shows errors from a failed job and from a rejected request", async () => {
    render(ToolView, { tool: onFile, files: ["/a.json"] });
    await fireEvent.click(screen.getByRole("button", { name: /Run/ }));
    emit({ status: "failed", error: { kind: "invalid_input", message: "line 1, column 2: unexpected end of input" } });
    expect((await screen.findByRole("alert")).textContent).toContain("line 1, column 2");

    api.runTool.mockRejectedValueOnce({ kind: "engine_missing", message: "not available yet" });
    await fireEvent.click(screen.getByRole("button", { name: /Run/ }));
    expect((await screen.findByRole("alert")).textContent).toContain("Not available yet");
  });

  it("ignores events from other jobs", async () => {
    render(ToolView, { tool: onFile, files: ["/a.json"] });
    await fireEvent.click(screen.getByRole("button", { name: /Run/ }));
    emit({ id: 99, status: "completed", output: { kind: "text", value: "other" } });
    expect(screen.queryByLabelText("Result")).toBeNull();
  });
});

describe("ToolView that needs no file", () => {
  it("has no Run: the result follows the text", async () => {
    render(ToolView, { tool: json, files: [] });
    expect(screen.queryByRole("button", { name: /Run/ })).toBeNull();
    expect(screen.getByText("Paste some text to start")).toBeTruthy();

    await fireEvent.input(screen.getByLabelText("Input"), { target: { value: "[1]" } });
    await vi.waitFor(() => expect(api.runTool).toHaveBeenCalledWith(json.id, { kind: "text", value: "[1]" }, expect.anything(), null));
    emit({ status: "completed", output: { kind: "text", value: "[\n  1\n]" } });
    expect(((await screen.findByLabelText("Result")) as HTMLTextAreaElement).value).toContain("1");
    expect(screen.queryByRole("status")).toBeNull();
  });

  it("runs once after a burst of typing", async () => {
    render(ToolView, { tool: json, files: [] });
    const input = screen.getByLabelText("Input");
    for (const value of ["[", "[1", "[1]"]) await fireEvent.input(input, { target: { value } });
    await vi.waitFor(() => expect(api.runTool).toHaveBeenCalledTimes(1));
    expect(api.runTool.mock.calls[0]![1]).toEqual({ kind: "text", value: "[1]" });
  });

  it("runs again when a setting changes", async () => {
    render(ToolView, { tool: json, files: [] });
    await fireEvent.input(screen.getByLabelText("Input"), { target: { value: "[1]" } });
    await vi.waitFor(() => expect(api.runTool).toHaveBeenCalledTimes(1));
    await fireEvent.click(screen.getByRole("radio", { name: "Minify" }));
    await vi.waitFor(() => expect(api.runTool).toHaveBeenCalledTimes(2));
  });

  it("keeps the last result while the next one is made, and stops the old job", async () => {
    render(ToolView, { tool: json, files: [] });
    const input = screen.getByLabelText("Input");
    await fireEvent.input(input, { target: { value: "[1]" } });
    await vi.waitFor(() => expect(api.runTool).toHaveBeenCalledTimes(1));
    emit({ status: "running" });
    await fireEvent.input(input, { target: { value: "[2]" } });
    await vi.waitFor(() => expect(api.cancelJob).toHaveBeenCalledWith(7));
  });

  it("clears the result with the text", async () => {
    render(ToolView, { tool: json, files: [] });
    const input = screen.getByLabelText("Input");
    await fireEvent.input(input, { target: { value: "[1]" } });
    await vi.waitFor(() => expect(api.runTool).toHaveBeenCalledTimes(1));
    emit({ status: "completed", output: { kind: "text", value: "x" } });
    await screen.findByLabelText("Result");
    await fireEvent.input(input, { target: { value: "" } });
    await vi.waitFor(() => expect(screen.queryByLabelText("Result")).toBeNull());
  });

  it("shows what is wrong with the text in place of the result", async () => {
    render(ToolView, { tool: json, files: [] });
    await fireEvent.input(screen.getByLabelText("Input"), { target: { value: "{" } });
    await vi.waitFor(() => expect(api.runTool).toHaveBeenCalledTimes(1));
    emit({ status: "failed", error: { kind: "invalid_input", message: "line 1, column 2: unexpected end of input" } });
    expect((await screen.findByRole("alert")).textContent).toContain("line 1, column 2");
  });

  it("makes something at once when it needs no input, and can make another", async () => {
    const tool = { ...json, inputs: [{ type: "none" }], presets: [{ id: "d", name: "D" }], parameters: [], ui: {} };
    const { container } = render(ToolView, { tool, files: [] });
    expect(container.querySelector("textarea")).toBeNull();
    await vi.waitFor(() => expect(api.runTool).toHaveBeenCalledWith(tool.id, { kind: "text", value: "" }, {}, null));
    await fireEvent.click(await screen.findByRole("button", { name: "Make another" }));
    await vi.waitFor(() => expect(api.runTool).toHaveBeenCalledTimes(2));
  });

  it("works on empty text where the tool allows it", async () => {
    render(ToolView, { tool: { ...json, inputs: [{ type: "text", optional: true }] }, files: [] });
    await vi.waitFor(() => expect(api.runTool).toHaveBeenCalledOnce());
  });

  it("counts as used once, not on every key", async () => {
    const onRan = vi.fn();
    render(ToolView, { tool: json, files: [], onRan });
    const input = screen.getByLabelText("Input");
    await fireEvent.input(input, { target: { value: "[1]" } });
    await vi.waitFor(() => expect(onRan).toHaveBeenCalledTimes(1));
    await fireEvent.input(input, { target: { value: "[2]" } });
    await vi.waitFor(() => expect(api.runTool).toHaveBeenCalledTimes(2));
    expect(onRan).toHaveBeenCalledTimes(1);
  });
});

describe("ToolView comparing", () => {
  const shrink = tool("image.compress", "Compress Images", {
    inputs: [{ type: "image", extensions: ["jpg"] }],
    parameters: [{ id: "quality", type: "integer", min: 1, max: 100, default: 75 }],
    presets: [{ id: "balanced", name: "Balanced", params: { quality: 75 } }, { id: "small", name: "Smaller", params: { quality: 40 } }],
    ui: { preview: "compare", advanced: ["quality"] },
  });

  beforeEach(() => {
    api.imagePreview.mockResolvedValue({ data: "AA", width: 10, height: 10 });
    api.previewTool.mockReset().mockImplementation(async (_: string, __: string, params: { quality: number }) => ({ data: "BB", size: params.quality * 1024, source_size: 1024 * 100 }));
  });

  it("shows the result and its size before anything is saved", async () => {
    render(ToolView, { tool: shrink, files: ["/a.jpg"] });
    expect(await screen.findByAltText("Result")).toBeTruthy();
    expect(await screen.findByText(/Result 75 KB/)).toBeTruthy();
    expect(screen.getByText(/−25%/)).toBeTruthy();
    expect(api.runTool).not.toHaveBeenCalled();
  });

  it("puts what each preset gives on the preset", async () => {
    render(ToolView, { tool: shrink, files: ["/a.jpg"] });
    expect(await screen.findByRole("radio", { name: /Smaller.*40 KB/ })).toBeTruthy();
  });

  it("follows the settings", async () => {
    render(ToolView, { tool: shrink, files: ["/a.jpg"] });
    await screen.findByText(/Result 75 KB/);
    await fireEvent.click(screen.getByRole("radio", { name: /Smaller/ }));
    expect(await screen.findByText(/Result 40 KB/)).toBeTruthy();
  });

  it("says what is wrong instead of showing nothing", async () => {
    api.previewTool.mockReset().mockRejectedValue({ kind: "invalid_input", message: "this picture is damaged" });
    render(ToolView, { tool: shrink, files: ["/a.jpg"] });
    expect(await screen.findByText("This picture is damaged")).toBeTruthy();
  });

  it("moves the divider with the keyboard", async () => {
    render(ToolView, { tool: shrink, files: ["/a.jpg"] });
    const divider = await screen.findByRole("slider");
    await fireEvent.keyDown(divider, { key: "ArrowRight" });
    expect(divider.getAttribute("aria-valuenow")).toBe("55");
  });
});

describe("ToolView on a timeline", () => {
  const trim = tool("video.trim", "Trim Video", {
    inputs: [{ type: "video", extensions: ["mp4"] }],
    parameters: [{ id: "start", type: "string", optional: true }, { id: "end", type: "string", optional: true }],
    ui: { preview: "timeline", one_of: ["start", "end"], advanced: ["start", "end"] },
  });

  beforeEach(() => {
    api.mediaTimeline.mockResolvedValue({ duration: 100, frames: ["AA", "BB"], peaks: [0.5, 1] });
  });

  it("asks for a part, then cuts what the edges show", async () => {
    render(ToolView, { tool: trim, files: ["/a.mp4"] });
    const start = await screen.findByRole("slider", { name: "Start" });
    expect(screen.getByText("Drag the edges to choose a part")).toBeTruthy();
    await fireEvent.keyDown(start, { key: "ArrowRight", shiftKey: true });
    await screen.findByText("1:30");
    await fireEvent.click(screen.getByRole("button", { name: /Run/ }));
    await vi.waitFor(() => expect(api.runTool).toHaveBeenCalledWith(trim.id, expect.anything(), { start: "0:10" }, null));
  });

  it("keeps the start before the end", async () => {
    render(ToolView, { tool: trim, files: ["/a.mp4"] });
    const end = await screen.findByRole("slider", { name: "End" });
    await fireEvent.keyDown(end, { key: "ArrowLeft" });
    expect(end.getAttribute("aria-valuetext")).toBe("1:39");
  });

  it("says when a file cannot be read", async () => {
    api.mediaTimeline.mockRejectedValue({ kind: "unsupported_format", message: "this file is damaged or is not a media file" });
    render(ToolView, { tool: trim, files: ["/a.mp4"] });
    expect(await screen.findByText("This file is damaged or is not a media file")).toBeTruthy();
  });
});

describe("ToolView reports", () => {
  it("shows sizes as bars, not as a log", async () => {
    render(ToolView, { tool: onFile, files: ["/a"] });
    await fireEvent.click(screen.getByRole("button", { name: /Run/ }));
    emit({ status: "completed", output: { kind: "text", value: "a: 3.0 GB in 2 files\n\n    2.0 GB  Video\n    1.0 GB  Photos" } });
    expect(await screen.findByRole("list", { name: "Sizes" })).toBeTruthy();
    expect(screen.queryByLabelText("Result")).toBeNull();
  });

  it("offers places as places", async () => {
    const mark = tool("image.watermark", "Watermark", {
      inputs: [{ type: "image", extensions: ["jpg"] }],
      parameters: [{ id: "position", type: "enum", values: ["top-left", "center", "bottom-right"], default: "center" }],
    });
    render(ToolView, { tool: mark, files: [] });
    await fireEvent.click(screen.getByRole("radio", { name: "Bottom right" }));
    expect(screen.getByRole("radio", { name: "Bottom right" }).getAttribute("aria-checked")).toBe("true");
  });
});

describe("ToolView turning pages", () => {
  const turn = tool("pdf.rotate-pages", "Rotate PDF Pages", {
    inputs: [{ type: "pdf", extensions: ["pdf"] }],
    parameters: [
      { id: "angle", type: "enum", values: ["90", "180", "270"], default: "90" },
      { id: "pages", type: "string", optional: true },
      { id: "turns", type: "string", optional: true },
    ],
    presets: [{ id: "right", name: "Right", params: { angle: "90" } }, { id: "left", name: "Left", params: { angle: "270" } }],
    ui: { preview: "pages-turn", advanced: ["turns"] },
  });

  beforeEach(() => {
    api.pdfThumbnails.mockResolvedValue({ pages: ["AA", "AA", "AA"], count: 3 });
  });

  it("turns a page on the page, without a form", async () => {
    render(ToolView, { tool: turn, files: ["/a.pdf"] });
    await fireEvent.click(await screen.findByRole("button", { name: "Page 2" }));
    expect(screen.getByRole("button", { name: "Page 2, turned 90 degrees" })).toBeTruthy();
    expect(screen.queryByRole("radio", { name: "Right" })).toBeNull();
    expect(screen.queryByRole("combobox")).toBeNull();

    await fireEvent.click(screen.getByRole("button", { name: /Run/ }));
    await vi.waitFor(() => expect(api.runTool).toHaveBeenCalledWith(turn.id, expect.anything(), expect.objectContaining({ turns: "2:90" }), null));
  });

  it("will not run until something is turned", async () => {
    render(ToolView, { tool: turn, files: ["/a.pdf"] });
    await screen.findByRole("button", { name: "Page 1" });
    expect(screen.getByRole("button", { name: /Run/ }).hasAttribute("disabled")).toBe(true);
  });

  it("turns back with Shift, and all pages at once", async () => {
    render(ToolView, { tool: turn, files: ["/a.pdf"] });
    await fireEvent.click(await screen.findByRole("button", { name: "Page 1" }));
    await fireEvent.click(screen.getByRole("button", { name: "Page 1, turned 90 degrees" }), { shiftKey: true });
    expect(screen.getByRole("button", { name: "Page 1" })).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Turn all right" }));
    expect(screen.getByText("3 pages turned")).toBeTruthy();
  });

  it("falls back to the form for several files", () => {
    render(ToolView, { tool: turn, files: ["/a.pdf", "/b.pdf"] });
    expect(screen.getByRole("radio", { name: "Right" })).toBeTruthy();
  });
});

describe("ToolView for cropping", () => {
  const crop = tool("image.crop", "Crop Images", {
    inputs: [{ type: "image", extensions: ["png"] }],
    parameters: [
      { id: "aspect", type: "enum", values: ["selection", "1:1"], default: "selection" },
      { id: "x", type: "integer", default: 0 },
      { id: "y", type: "integer", default: 0 },
      { id: "width", type: "integer", optional: true },
      { id: "height", type: "integer", optional: true },
    ],
    presets: [
      { id: "selection", name: "Selection", params: { aspect: "selection" } },
      { id: "square", name: "Square", params: { aspect: "1:1" } },
    ],
    ui: { preview: "crop" },
  });

  it("turns a drag on the picture into the area it runs with", async () => {
    api.imagePreview.mockResolvedValue({ data: "AA==", width: 400, height: 200 });
    render(ToolView, { tool: crop, files: ["/a.png"] });
    const stage = (await screen.findByAltText("Preview")).parentElement!;
    stage.getBoundingClientRect = () => ({ left: 0, top: 0, width: 200, height: 100 }) as DOMRect;

    await fireEvent.pointerDown(stage, { clientX: 10, clientY: 10 });
    await fireEvent.pointerMove(stage, { clientX: 60, clientY: 35 });
    await fireEvent.pointerUp(stage);
    expect(screen.getByText("100 × 50 of 400 × 200")).toBeTruthy();

    await fireEvent.click(screen.getByRole("button", { name: /Run/ }));
    expect(api.runTool).toHaveBeenCalledWith(
      "image.crop",
      { kind: "files", value: ["/a.png"] },
      { aspect: "selection", x: 20, y: 20, width: 100, height: 50 },
      null,
    );
  });

  it("shows the shape a preset will cut", async () => {
    api.imagePreview.mockResolvedValue({ data: "AA==", width: 400, height: 200 });
    render(ToolView, { tool: crop, files: ["/a.png"] });
    await screen.findByAltText("Preview");
    await fireEvent.click(screen.getByRole("radio", { name: "Square" }));
    expect(screen.getByText("200 × 200 of 400 × 200")).toBeTruthy();
  });

  it("sends results to the chosen folder", async () => {
    render(ToolView, { tool: compress, files: ["/a.jpg"], outputDir: "/out" });
    await fireEvent.click(screen.getByRole("button", { name: /Run/ }));
    expect(api.runTool.mock.calls[0]![3]).toBe("/out");
  });
});

describe("ToolView with page pictures", () => {
  const pages = { pages: ["AA==", "AA==", "AA=="], count: 3 };
  const deletePages = tool("pdf.delete-pages", "Delete PDF Pages", {
    inputs: [{ type: "pdf", extensions: ["pdf"] }],
    parameters: [{ id: "pages", type: "string", optional: true }],
    ui: { preview: "pages-select" },
  });
  const reorder = tool("pdf.reorder-pages", "Reorder PDF Pages", {
    inputs: [{ type: "pdf", extensions: ["pdf"] }],
    parameters: [{ id: "order", type: "string", optional: true }],
    ui: { preview: "pages-order" },
  });

  it("fills in the pages that were clicked", async () => {
    api.pdfThumbnails.mockResolvedValue(pages);
    render(ToolView, { tool: deletePages, files: ["/a.pdf"] });
    await fireEvent.click(await screen.findByRole("button", { name: "Page 1" }));
    await fireEvent.click(screen.getByRole("button", { name: "Page 2" }));
    expect((screen.getByRole("textbox", { name: "Pages" }) as HTMLInputElement).value).toBe("1-2");
    await fireEvent.click(screen.getByRole("button", { name: "Page 1" }));
    expect((screen.getByRole("textbox", { name: "Pages" }) as HTMLInputElement).value).toBe("2");
  });

  it("follows pages typed by hand", async () => {
    api.pdfThumbnails.mockResolvedValue(pages);
    render(ToolView, { tool: deletePages, files: ["/a.pdf"] });
    const page = await screen.findByRole("button", { name: "Page 3" });
    await fireEvent.input(screen.getByRole("textbox", { name: "Pages" }), { target: { value: "3" } });
    expect(page.getAttribute("aria-pressed")).toBe("true");
  });

  it("reorders with the keyboard", async () => {
    api.pdfThumbnails.mockResolvedValue(pages);
    render(ToolView, { tool: reorder, files: ["/a.pdf"] });
    await fireEvent.keyDown(await screen.findByRole("button", { name: "Page 1" }), { key: "ArrowRight" });
    expect((screen.getByRole("textbox", { name: "Order" }) as HTMLInputElement).value).toBe("2,1,3");
  });

  it("still works when pages cannot be shown", async () => {
    api.pdfThumbnails.mockRejectedValue({ kind: "unsupported_format", message: "no" });
    render(ToolView, { tool: deletePages, files: ["/locked.pdf"] });
    await Promise.resolve();
    expect(screen.queryByRole("group", { name: "Pages" })).toBeNull();
    expect(screen.getByRole("textbox", { name: "Pages" })).toBeTruthy();
  });

  it("shows no pictures for several files", () => {
    render(ToolView, { tool: deletePages, files: ["/a.pdf", "/b.pdf"] });
    expect(api.pdfThumbnails).not.toHaveBeenCalledWith("/a.pdf");
  });
});

describe("ToolView helps with what is missing", () => {
  const pick = tool("pdf.extract-pages", "Extract Pages", {
    inputs: [{ type: "pdf", extensions: ["pdf"] }],
    parameters: [{ id: "pages", type: "string", optional: true }],
    ui: { one_of: ["pages"] },
  });

  it("will not run without a field the tool needs, and says so", async () => {
    render(ToolView, { tool: pick, files: ["/a.pdf"] });
    expect(screen.getByText("Fill in Pages")).toBeTruthy();
    expect(screen.getByRole("button", { name: /Run/ }).hasAttribute("disabled")).toBe(true);
    await fireEvent.input(screen.getByRole("textbox"), { target: { value: "1" } });
    expect(screen.getByRole("button", { name: /Run/ }).hasAttribute("disabled")).toBe(false);
  });

  it("says when a number is out of range, in words", async () => {
    render(ToolView, { tool: compress, files: ["/a.jpg"] });
    await fireEvent.input(screen.getByRole("spinbutton", { hidden: true }), { target: { value: "500" } });
    expect(screen.getAllByText("Must be from 1 to 100").length).toBeGreaterThan(0);
    expect(screen.getByRole("button", { name: /Run/ }).hasAttribute("disabled")).toBe(true);
  });

  it("explains why page pictures are gone with two files", () => {
    const pages = tool("pdf.delete-pages", "Delete Pages", {
      inputs: [{ type: "pdf", extensions: ["pdf"] }],
      parameters: [{ id: "pages", type: "string", optional: true }],
      ui: { preview: "pages-select" },
    });
    render(ToolView, { tool: pages, files: ["/a.pdf", "/b.pdf"] });
    expect(screen.getByText("Page pictures show when only one file is chosen.")).toBeTruthy();
  });

  it("marks the preset the fields stand at", async () => {
    render(ToolView, { tool: onFile, files: [] });
    await fireEvent.change(screen.getByRole("combobox", { hidden: true }), { target: { value: "minify" } });
    expect(screen.getByRole("radio", { name: "Minify" }).getAttribute("aria-checked")).toBe("true");
  });
});

describe("ToolView crop areas", () => {
  const crop = tool("image.crop", "Crop", {
    inputs: [{ type: "image", extensions: ["jpg"] }],
    parameters: [
      { id: "aspect", type: "enum", values: ["selection", "1:1"], default: "selection" },
      { id: "x", type: "integer", default: 0 },
      { id: "y", type: "integer", default: 0 },
      { id: "width", type: "integer", optional: true },
      { id: "height", type: "integer", optional: true },
    ],
    presets: [
      { id: "selection", name: "Selection", params: { aspect: "selection" } },
      { id: "square", name: "Square", params: { aspect: "1:1" } },
    ],
    ui: { preview: "crop", advanced: ["aspect", "x", "y", "width", "height"] },
  });

  it("asks for an area instead of running a crop that cuts nothing", () => {
    render(ToolView, { tool: crop, files: ["/a.jpg"] });
    expect(screen.getByText("Choose an area on the picture")).toBeTruthy();
    expect(screen.getByRole("button", { name: /Run/ }).hasAttribute("disabled")).toBe(true);
  });

  it("keeps the selection when going back to it from a shape", async () => {
    render(ToolView, { tool: crop, files: ["/a.jpg"] });
    const [, , width] = screen.getAllByRole("spinbutton", { hidden: true });
    await fireEvent.input(width!, { target: { value: "10" } });
    await fireEvent.click(screen.getByRole("radio", { name: "Selection" }));
    expect((width as HTMLInputElement).value).toBe("10");
  });
});

describe("ToolView and folders", () => {
  const size = tool("files.folder-size", "Folder Size", { inputs: [{ type: "any", extensions: ["*"] }] });

  it("lets a tool for anything take a folder without dragging", async () => {
    api.pickFolder.mockResolvedValue("/photos");
    render(ToolView, { tool: size, files: [] });
    await fireEvent.click(screen.getByRole("button", { name: "Choose folder…" }));
    expect(await screen.findByText("photos")).toBeTruthy();
  });

  it("offers no folders to a tool that takes images", () => {
    render(ToolView, { tool: compress, files: [] });
    expect(screen.queryByRole("button", { name: "Choose folder…" })).toBeNull();
  });
});

describe("ToolView out of sight", () => {
  it("does not run on the shortcut", async () => {
    render(ToolView, { tool: compress, files: ["/a.jpg"], active: false });
    await fireEvent.keyDown(window, { key: "Enter", metaKey: true });
    expect(api.runTool).not.toHaveBeenCalled();
  });

  it("puts the keyboard on the tool when it opens", () => {
    render(ToolView, { tool: compress, files: [] });
    expect(document.activeElement).toBe(screen.getByRole("heading", { name: compress.name }));
  });

  it("does not miss a job that ends before its id comes back", async () => {
    api.runTool.mockImplementation(async () => {
      emit({ status: "completed", output: { kind: "text", value: "early" } });
      return 7;
    });
    render(ToolView, { tool: onFile, files: ["/a.json"] });
    await fireEvent.click(screen.getByRole("button", { name: /Run/ }));
    expect(((await screen.findByLabelText("Result")) as HTMLTextAreaElement).value).toBe("early");
  });
});
