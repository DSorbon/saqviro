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
    await fireEvent.change(screen.getByRole("combobox", { name: "Add a step" }), { target: { value: "image.to-webp" } });
    expect(current().steps).toEqual([{ tool: "image.to-webp", params: { quality: 80 } }]);
  });

  it("only offers tools that can use the last result", () => {
    show(workflow([{ tool: "image.resize" }]));
    const names = screen.getAllByRole("option").map((o) => o.textContent);
    expect(names).toEqual(["Choose a tool…", "Resize", "To WebP"]);
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

  it("renames", async () => {
    const { current } = show(workflow([]));
    await fireEvent.input(screen.getByRole("textbox", { name: "Workflow name" }), { target: { value: "Mine" } });
    expect(current().name).toBe("Mine");
  });
});
