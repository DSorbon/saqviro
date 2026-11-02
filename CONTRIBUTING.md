# Contributing to Saqviro

Thanks for helping. A few rules keep the project healthy.

## Commits

We use [Conventional Commits](https://www.conventionalcommits.org). A `commit-msg` hook checks every message.

```text
type(scope): subject
```

- **types:** `feat`, `fix`, `docs`, `test`, `refactor`, `perf`, `build`, `ci`, `chore`, `revert`
- **scope:** a crate or package (`core`, `desktop`, `tool-schema`) or a tool id (`pdf.merge`, `image.resize`)
- **subject:** imperative, lowercase, no trailing period, header ≤ 72 characters
- breaking changes to the manifest or workflow schema need a `BREAKING CHANGE:` footer

PRs are squash-merged; the PR title becomes the commit message.

## Tests are required

No code is merged without tests.

| Change | Required tests |
|---|---|
| New tool | Rust unit tests on fixture files: success, empty input, unsupported format, cancel |
| Core logic | Unit tests, plus an integration test against a temp directory when it touches the file system |
| Frontend logic | Vitest |
| Bug fix | A test that fails without the fix |
| Manifest or schema | Covered by `crates/core` tests — every `tools/**/tool.yaml` must load |

Write the test first or together with the code. A flaky test is a bug, not something to `#[ignore]`.

## Before you push

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm check && pnpm test
```

## Adding a tool

1. Create `tools/<category>/<name>/tool.yaml`. Your editor validates it against `packages/tool-schema/tool.schema.json`.
2. The id is `category.operation[.qualifier]` and never changes after release.
3. Give it 1–4 presets; the first is the default. Hide rarely used parameters with `ui.advanced`.
4. If you change the manifest types in `crates/core`, regenerate the schema:
   `UPDATE_SCHEMA=1 cargo test -p saqviro-core --test schema`

## Language

Everything in this repository — code, comments, commits, issues, docs, UI text — is in English.

## Licensing

By contributing you agree that your work is licensed under Apache-2.0. Do not add dependencies under GPL or AGPL; `cargo-deny` checks this in CI.
