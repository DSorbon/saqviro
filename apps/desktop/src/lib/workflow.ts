import starterData from "../../../../packages/workflow-schema/starters.json";
import { inputKind } from "./job";
import { initialParams } from "./params";
import type { ParamValue, Tool } from "./types";

// Mirrors `saqviro_workflow`. The Rust types are the source of truth.

export interface WorkflowStep {
  tool: string;
  params?: Record<string, ParamValue>;
}

export interface Workflow {
  schema_version: number;
  id: string;
  name: string;
  steps: WorkflowStep[];
  keep_going?: boolean;
}

const SCHEMA_VERSION = 1;

export const starters = starterData as Workflow[];

const TEXT = ["text", "json"];

/** True if what `from` makes can go into `to`. Mirrors `can_follow`. */
export function canFollow(from: Tool, to: Tool): boolean {
  const made = from.outputs?.[0]?.type;
  const wanted = to.inputs[0]?.type;
  if (!made || !wanted || made === "none" || wanted === "none") return false;
  if (TEXT.includes(made) || TEXT.includes(wanted)) return TEXT.includes(made) && TEXT.includes(wanted);
  return wanted === "any" || made === wanted;
}

const inPlace = (tool: Tool) => tool.capabilities?.destructive === true;

/** Tools that can come after the last step. */
export function nextTools(tools: Tool[], steps: WorkflowStep[]): Tool[] {
  const lastId = steps.at(-1)?.tool;
  const last = tools.find((t) => t.id === lastId);
  const usable = tools.filter((t) => !inPlace(t));
  return steps.length === 0 || !last ? usable : usable.filter((t) => canFollow(last, t));
}

/** Index of the first step that cannot work, or -1 if all can. */
export function brokenStep(tools: Tool[], steps: WorkflowStep[]): number {
  const found = steps.map((s) => tools.find((t) => t.id === s.tool));
  return found.findIndex((tool, i) => {
    if (!tool) return true;
    const before = found[i - 1];
    return i > 0 && before !== undefined && !canFollow(before, tool);
  });
}

export const stepOf = (tool: Tool): WorkflowStep => ({ tool: tool.id, params: initialParams(tool) });

export function blankWorkflow(id: string): Workflow {
  return { schema_version: SCHEMA_VERSION, id, name: "New workflow", steps: [] };
}

export const copyOf = (workflow: Workflow, id: string): Workflow => ({
  ...(JSON.parse(JSON.stringify(workflow)) as Workflow),
  id,
  name: `${workflow.name} copy`,
});

/** Every step with a value for each setting, so the editor can show them. */
export function withDefaults(workflow: Workflow, tools: Tool[]): Workflow {
  const steps = workflow.steps.map((step) => {
    const tool = tools.find((t) => t.id === step.tool);
    return { ...step, params: { ...(tool ? initialParams(tool) : {}), ...step.params } };
  });
  return { ...workflow, steps };
}

/** Replaces the workflow with the same id, or adds it. */
export function saved(list: Workflow[], workflow: Workflow): Workflow[] {
  return list.some((w) => w.id === workflow.id)
    ? list.map((w) => (w.id === workflow.id ? workflow : w))
    : [...list, workflow];
}

/** What the first step takes: the workflow's own input. */
export function workflowInput(tools: Tool[], workflow: Workflow): "text" | "files" | "none" {
  const first = tools.find((t) => t.id === workflow.steps[0]?.tool);
  return first ? inputKind(first) : "none";
}

/** A workflow to share as text. */
export function exported(workflow: Workflow): string {
  return JSON.stringify(workflow, null, 2);
}

/** A workflow from shared text, under a new id, or null if it is not one. */
export function imported(text: string, id: string): Workflow | null {
  let value: unknown;
  try {
    value = JSON.parse(text);
  } catch {
    return null;
  }
  return isWorkflow(value) ? { ...value, id } : null;
}

function isStep(value: unknown): value is WorkflowStep {
  return typeof (value as WorkflowStep | null)?.tool === "string";
}

export function isWorkflow(value: unknown): value is Workflow {
  const w = value as Workflow | null;
  return typeof w?.id === "string" && typeof w?.name === "string" && Array.isArray(w?.steps) && w.steps.every(isStep);
}
