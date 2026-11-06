# Workflows

A workflow runs tools one after another, each on what the last one made. It is one job: one progress bar, one Cancel, and only the final result is saved.

```json
{
  "schema_version": 1,
  "id": "website-images",
  "name": "Website Images",
  "steps": [
    { "tool": "image.resize", "params": { "width": 1920 } },
    { "tool": "image.metadata.remove" },
    { "tool": "image.to-webp", "params": { "quality": 80 } }
  ]
}
```

`packages/workflow-schema/workflow.schema.json` is generated from the Rust types in `crates/workflow`. `starters.json` next to it holds the workflows that ship with the app; a test runs them through the same checks as any other.

## What can follow what

A step can follow another when what the first makes is what the second takes, by the `outputs` and `inputs` of their manifests. Text goes to text, and anything file-based goes to a tool that takes any file. `saqviro_workflow::can_follow` is the rule; `canFollow` in `apps/desktop/src/lib/workflow.ts` mirrors it for the editor. The engine checks again before running.

- Several files in a step go through together, so one file can become many (PDF to Images) and many can become one (Images to PDF).
- A step that makes a folder hands the next step the files inside it.
- A generated picture, like a QR code, is written to a file for the next step.
- Tools marked `destructive` change files in place, so they cannot be part of a workflow.

## How it runs

`prepare` validates the steps and returns one `Executor`. Each step gets a scoped `JobContext` that shares the job's cancellation, reports progress within its share, and sends results to a folder inside the job's temp directory. Only the last step writes to the results folder: the one chosen in Settings, or else the folder of the first file.

The first failing step stops the job. Its message starts with the name of the tool, and nothing is left behind.

Workflows are saved with the app's preferences on this device.
