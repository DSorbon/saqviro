// @vitest-environment jsdom
import { fireEvent, render, screen } from "@testing-library/svelte";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { JobSnapshot } from "../lib/job";
import { tool } from "../lib/fixtures";
import { blankWorkflow, withDefaults, type Workflow } from "../lib/workflow";
import WorkflowView from "./WorkflowView.svelte";

const api = vi.hoisted(() => ({
  listeners: [] as ((s: JobSnapshot) => void)[],
  runWorkflow: vi.fn(),
  cancelJob: vi.fn(),
  pickFiles: vi.fn(),
}));

vi.mock("../lib/api", () => ({
  runWorkflow: api.runWorkflow,
  cancelJob: api.cancelJob,
  pickFiles: api.pickFiles,
  saveImage: vi.fn(),
  pickFolder: vi.fn(),
  openFile: vi.fn(),
  revealFile: vi.fn(),
  onJobEvent: (cb: (s: JobSnapshot) => void) => {
    api.listeners.push(cb);
    return Promise.resolve(() => {});
  },
}));

const resize = tool("image.resize", "Resize", {
  inputs: [{ type: "image" }],
  outputs: [{ type: "image" }],
  parameters: [{ id: "width", type: "integer", optional: true }],
});
const webp = tool("image.to-webp", "To WebP", {
  inputs: [{ type: "image" }],
  outputs: [{ type: "image" }],
  parameters: [{ id: "quality", type: "integer", default: 80 }],
});
const merge = tool("pdf.merge", "Merge PDF", { inputs: [{ type: "pdf" }], outputs: [{ type: "pdf" }] });
const upper = tool("text.case", "Case", { inputs: [{ type: "text" }], outputs: [{ type: "text" }] });
const tools = [resize, webp, merge, upper];

/** The app gives the view steps with their settings filled in. */
const workflow = (steps: Workflow["steps"]): Workflow =>
  withDefaults({ ...blankWorkflow("w"), steps }, tools);

/** Renders a view that keeps its changes, as the app does. */
function show(start: Workflow, files: string[] = []) {
  let current = start;
  const changes = vi.fn();
  const props = {
    tools,
    workflow: start,
    files,
    onChange: (next: Workflow) => {
      current = next;
      changes(next);
      void view.rerender({ ...props, workflow: next });
    },
    onDelete: vi.fn(),
    onCopy: vi.fn(),
    onBack: vi.fn(),
  };
  const view = render(WorkflowView, props);
  return { changes, props, current: () => current };
}

const emit = (over: Partial<JobSnapshot>) =>
  api.listeners.forEach((l) =>
    l({ id: 3, tool_id: "workflow", status: "running", progress: 0, output: null, error: null, started_at: 0, finished_at: null, ...over }),
  );

beforeEach(() => {
  api.listeners.length = 0;
  api.runWorkflow.mockReset().mockResolvedValue(3);
  api.pickFiles.mockReset().mockResolvedValue([]);
  api.cancelJob.mockReset().mockResolvedValue(true);
});

describe("WorkflowView", () => {
  it("adds a step with the tool's defaults", async () => {
    const { current } = show(workflow([]));
    await fireEvent.focus(screen.getByRole("searchbox", { name: "Add a step" }));
    await fireEvent.click(screen.getByRole("button", { name: "To WebP" }));
    expect(current().steps).toEqual([{ tool: "image.to-webp", params: { quality: 80 } }]);
  });

  it("only offers tools that can use the last result", async () => {
    show(workflow([{ tool: "image.resize" }]));
    await fireEvent.focus(screen.getByRole("searchbox", { name: "Add a step" }));
    expect(screen.getByRole("button", { name: "To WebP" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Merge PDF" })).toBeNull();
  });

  it("finds a step by searching, and adds the first match with Enter", async () => {
    const { current } = show(workflow([]));
    const search = screen.getByRole("searchbox", { name: "Add a step" });
    await fireEvent.input(search, { target: { value: "webp" } });
    expect(screen.queryByRole("button", { name: "Resize" })).toBeNull();
    await fireEvent.keyDown(search, { key: "Enter" });
    expect(current().steps.map((s) => s.tool)).toEqual(["image.to-webp"]);
  });

  it("offers a step's presets and shows the one in use", async () => {
    const scaled = tool("image.scale", "Scale", {
      inputs: [{ type: "image" }],
      outputs: [{ type: "image" }],
      parameters: [{ id: "width", type: "integer", default: 100 }],
      presets: [{ id: "s", name: "Small", params: { width: 100 } }, { id: "l", name: "Large", params: { width: 900 } }],
    });
    let current = withDefaults({ ...blankWorkflow("w"), steps: [{ tool: "image.scale" }] }, [scaled]);
    render(WorkflowView, {
      tools: [scaled], workflow: current, files: [],
      onChange: (w: Workflow) => (current = w), onDelete: vi.fn(), onCopy: vi.fn(), onBack: vi.fn(),
    });
    const preset = screen.getByRole("combobox", { name: "Preset of step 1" }) as HTMLSelectElement;
    expect(preset.value).toBe("s");
    await fireEvent.change(preset, { target: { value: "l" } });
    expect(current.steps[0]!.params).toEqual({ width: 900 });
  });

  it("says which step is missing a setting and will not run", () => {
    const needy = tool("pdf.pick", "Pick", {
      inputs: [{ type: "pdf" }],
      outputs: [{ type: "pdf" }],
      parameters: [{ id: "pages", type: "string", optional: true }],
      ui: { one_of: ["pages"] },
    });
    render(WorkflowView, {
      tools: [needy], workflow: withDefaults({ ...blankWorkflow("w"), steps: [{ tool: "pdf.pick" }] }, [needy]), files: ["/a.pdf"],
      onChange: vi.fn(), onDelete: vi.fn(), onCopy: vi.fn(), onBack: vi.fn(),
    });
    expect(screen.getByText("Step 1: Fill in Pages")).toBeTruthy();
    expect(screen.getByRole("button", { name: /Run/ }).hasAttribute("disabled")).toBe(true);
  });

  it("shows a picture a workflow ends with, and can save it", async () => {
    api.runWorkflow.mockResolvedValue(7);
    show(workflow([{ tool: "text.case" }]));
    await fireEvent.input(screen.getByLabelText("Input"), { target: { value: "x" } });
    await fireEvent.click(screen.getByRole("button", { name: /Run/ }));
    await vi.waitFor(() => expect(screen.queryByRole("button", { name: "Cancel" })).toBeTruthy());
    api.listeners.forEach((l) =>
      l({ id: 7, tool_id: "workflow", status: "completed", progress: 1, error: null, started_at: 0, finished_at: 1, output: { kind: "image", value: { ext: "png", data: "AA" } } }),
    );
    expect(await screen.findByRole("img", { name: "Result" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Save…" })).toBeTruthy();
  });

  it("cannot be deleted while it runs", async () => {
    api.runWorkflow.mockResolvedValue(7);
    show(workflow([{ tool: "text.case" }]));
    await fireEvent.input(screen.getByLabelText("Input"), { target: { value: "x" } });
    await fireEvent.click(screen.getByRole("button", { name: /Run/ }));
    await vi.waitFor(() => expect(screen.getByRole("button", { name: "Delete" }).hasAttribute("disabled")).toBe(true));
  });

  it("keeps the keyboard on a step after moving it", async () => {
    show(workflow([{ tool: "image.resize" }, { tool: "image.to-webp" }, { tool: "image.resize" }]));
    await fireEvent.click(screen.getByRole("button", { name: "Move step 2 up" }));
    await vi.waitFor(() => expect(document.activeElement?.getAttribute("aria-label")).toBe("Move step 1 down"));
  });

  it("moves and removes steps", async () => {
    const { current } = show(workflow([{ tool: "image.resize" }, { tool: "image.to-webp" }]));
    await fireEvent.click(screen.getByRole("button", { name: "Move step 2 up" }));
    expect(current().steps.map((s) => s.tool)).toEqual(["image.to-webp", "image.resize"]);
    await fireEvent.click(screen.getByRole("button", { name: "Remove step 1" }));
    expect(current().steps.map((s) => s.tool)).toEqual(["image.resize"]);
  });

  it("marks a step that does not fit and will not run", () => {
    show(workflow([{ tool: "image.resize" }, { tool: "pdf.merge" }]), ["/a.png"]);
    expect(screen.getByText(/Merge PDF cannot use/)).toBeTruthy();
    expect((screen.getByRole("button", { name: /Run/ }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("runs on dropped files and shows the result", async () => {
    show(workflow([{ tool: "image.resize", params: { width: 100 } }]), ["/a.png"]);
    await fireEvent.click(screen.getByRole("button", { name: /Run/ }));
    expect(api.runWorkflow).toHaveBeenCalledWith(
      [{ tool: "image.resize", params: { width: 100 } }],
      { kind: "files", value: ["/a.png"] },
      null,
      false,
    );
    await Promise.resolve();
    emit({ status: "completed", progress: 1, output: { kind: "files", value: [{ path: "/a-resized.png", size: 9, source_size: 18 }] } });
    expect(await screen.findByText("a-resized.png")).toBeTruthy();
  });

  it("takes text when its first step does", async () => {
    show(workflow([{ tool: "text.case" }]));
    await fireEvent.input(screen.getByRole("textbox", { name: "Input" }), { target: { value: "hi" } });
    await fireEvent.click(screen.getByRole("button", { name: /Run/ }));
    expect(api.runWorkflow.mock.calls[0]![1]).toEqual({ kind: "text", value: "hi" });
  });

  it("shows why a run was refused", async () => {
    api.runWorkflow.mockRejectedValue({ kind: "invalid_input", message: "step 1: width is wrong" });
    show(workflow([{ tool: "image.resize" }]), ["/a.png"]);
    await fireEvent.click(screen.getByRole("button", { name: /Run/ }));
    expect((await screen.findByRole("alert")).textContent).toContain("step 1: width is wrong");
  });

  it("keeps changed settings", async () => {
    const { current } = show(workflow([{ tool: "image.resize", params: {} }]));
    await fireEvent.input(screen.getByRole("spinbutton", { name: "Width" }), { target: { value: "640" } });
    expect(current().steps[0]!.params).toEqual({ width: 640 });
  });

  it("can go on past a file that fails", async () => {
    const { current } = show(workflow([{ tool: "image.resize" }]), ["/a.png"]);
    await fireEvent.click(screen.getByRole("checkbox", { name: /go on with the others/ }));
    expect(current().keep_going).toBe(true);
    await fireEvent.click(screen.getByRole("button", { name: /Run/ }));
    expect(api.runWorkflow.mock.calls[0]![3]).toBe(true);
  });

  it("renames", async () => {
    const { current } = show(workflow([]));
    await fireEvent.input(screen.getByRole("textbox", { name: "Workflow name" }), { target: { value: "Mine" } });
    expect(current().name).toBe("Mine");
  });
});
