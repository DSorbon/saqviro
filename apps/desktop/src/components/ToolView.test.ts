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
}));

vi.mock("../lib/api", () => ({
  runTool: api.runTool,
  cancelJob: api.cancelJob,
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
});

describe("ToolView", () => {
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

    expect(api.runTool).toHaveBeenCalledWith(json.id, { kind: "text", value: '{"a":1}' }, { indent: "minify", sort_keys: false });

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
    expect((await screen.findByRole("alert")).textContent).toContain("not available yet");
  });

  it("ignores events from other jobs", async () => {
    render(ToolView, { tool: json, files: [] });
    await fireEvent.input(screen.getByLabelText("Input"), { target: { value: "[]" } });
    await fireEvent.click(screen.getByRole("button", { name: /Run/ }));
    emit({ id: 99, status: "completed", output: { kind: "text", value: "other" } });
    expect(screen.queryByLabelText("Result")).toBeNull();
  });
});
