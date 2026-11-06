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
  revealFile: vi.fn(),
  openFile: vi.fn(),
  saveImage: vi.fn(),
  imagePreview: vi.fn(),
  pdfThumbnails: vi.fn(),
}));

vi.mock("../lib/api", () => ({
  runTool: api.runTool,
  cancelJob: api.cancelJob,
  pickFiles: api.pickFiles,
  revealFile: api.revealFile,
  openFile: api.openFile,
  saveImage: api.saveImage,
  imagePreview: api.imagePreview,
  pdfThumbnails: api.pdfThumbnails,
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
  it("runs a tool that takes no input", async () => {
    const tool = { ...json, inputs: [{ type: "none" }], presets: [{ id: "d", name: "D" }], parameters: [], ui: {} };
    const { container } = render(ToolView, { tool, files: [] });
    expect(container.querySelector("textarea")).toBeNull();
    expect(screen.queryByText("Drop files on this window")).toBeNull();
    await fireEvent.click(screen.getByRole("button", { name: /Run/ }));
    expect(api.runTool).toHaveBeenCalledWith(tool.id, { kind: "text", value: "" }, {}, null);
  });

  it("allows optional text to stay empty", () => {
    const tool = { ...json, inputs: [{ type: "text", optional: true }] };
    render(ToolView, { tool, files: [] });
    expect(screen.getByRole("button", { name: /Run/ }).hasAttribute("disabled")).toBe(false);
  });

  it("shows an image result and saves it", async () => {
    render(ToolView, { tool: json, files: [] });
    await fireEvent.input(screen.getByLabelText("Input"), { target: { value: "x" } });
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
    render(ToolView, { tool: json, files: [] });
    await fireEvent.input(screen.getByLabelText("Input"), { target: { value: "x" } });
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
    render(ToolView, { tool: json, files: [] });
    expect(screen.getByRole("radio", { name: "Pretty" }).getAttribute("aria-checked")).toBe("true");
    expect(screen.getByText("Advanced")).toBeTruthy();
    expect(screen.getByRole("button", { name: /Run/ }).hasAttribute("disabled")).toBe(true);
  });

  it("runs with the chosen preset and shows the result", async () => {
    render(ToolView, { tool: json, files: [] });
    await fireEvent.input(screen.getByLabelText("Input"), { target: { value: '{"a":1}' } });
    await fireEvent.click(screen.getByRole("radio", { name: "Minify" }));
    await fireEvent.click(screen.getByRole("button", { name: /Run/ }));

    expect(api.runTool).toHaveBeenCalledWith(json.id, { kind: "text", value: '{"a":1}' }, { indent: "minify", sort_keys: false }, null);

    emit({ status: "running", progress: 0.5 });
    expect(await screen.findByText("Working… 50%")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Cancel" })).toBeTruthy();

    emit({ status: "completed", progress: 1, finished_at: 200, output: { kind: "text", value: '{"a":1}' } });
    expect(await screen.findByText("Done in 200 ms")).toBeTruthy();
    expect((screen.getByLabelText("Result") as HTMLTextAreaElement).value).toBe('{"a":1}');
  });

  it("runs with Cmd/Ctrl+Enter, but only when the input is ready", async () => {
    render(ToolView, { tool: json, files: [] });
    await fireEvent.keyDown(window, { key: "Enter", metaKey: true });
    expect(api.runTool).not.toHaveBeenCalled();

    await fireEvent.input(screen.getByLabelText("Input"), { target: { value: "[]" } });
    await fireEvent.keyDown(window, { key: "Enter", ctrlKey: true });
    expect(api.runTool).toHaveBeenCalledTimes(1);
  });

  it("cancels a running job", async () => {
    render(ToolView, { tool: json, files: [] });
    await fireEvent.input(screen.getByLabelText("Input"), { target: { value: "[]" } });
    await fireEvent.click(screen.getByRole("button", { name: /Run/ }));
    emit({ status: "running" });
    await fireEvent.click(await screen.findByRole("button", { name: "Cancel" }));
    expect(api.cancelJob).toHaveBeenCalledWith(7);
  });

  it("shows errors from a failed job and from a rejected request", async () => {
    render(ToolView, { tool: json, files: [] });
    await fireEvent.input(screen.getByLabelText("Input"), { target: { value: "{" } });
    await fireEvent.click(screen.getByRole("button", { name: /Run/ }));
    emit({ status: "failed", error: { kind: "invalid_input", message: "line 1, column 2: unexpected end of input" } });
    expect((await screen.findByRole("alert")).textContent).toContain("line 1, column 2");

    api.runTool.mockRejectedValueOnce({ kind: "engine_missing", message: "not available yet" });
    await fireEvent.click(screen.getByRole("button", { name: /Run/ }));
    expect((await screen.findByRole("alert")).textContent).toContain("Not available yet");
  });

  it("ignores events from other jobs", async () => {
    render(ToolView, { tool: json, files: [] });
    await fireEvent.input(screen.getByLabelText("Input"), { target: { value: "[]" } });
    await fireEvent.click(screen.getByRole("button", { name: /Run/ }));
    emit({ id: 99, status: "completed", output: { kind: "text", value: "other" } });
    expect(screen.queryByLabelText("Result")).toBeNull();
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
