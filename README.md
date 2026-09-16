# Hireling

Party-linked Pathfinder 2e character tracker. Pathbuilder imports, live buff
propagation across the party, shared inventory, PWA.

Status: scaffolding. Spec: `docs/PRD.md`. Law: `CONSTITUTION.md`.
Agent rules: `AGENTS.md` + `docs/toolkit-conventions.md`.

## Development

```sh
cargo run                                                 # run the server
cargo test --quiet                                        # tests
cargo fmt --all                                           # format
cargo clippy --all-targets --all-features -- -D warnings  # lint
cargo deny check                                          # audit dependencies
```

With [just](https://github.com/casey/just) installed, `just ci-local` runs the
full local gate. Git hooks enforce the same checks on every commit — install
them once per clone with `./.githooks/install.sh`.

CI runs [grizzly-gate](https://github.com/Grizzly-Endeavors/grizzly-gate)
(standalone mode) on every PR. The gate is the first reviewer; a human is the
second.

## Layout

- `src/` — the Rust backend (axum, single binary)
- `web/` — the Svelte PWA (lands with the frontend skeleton)
- `docs/` — PRD, checker reports, ADRs (`docs/decisions/`), vendored tooling
  references (`toolkit.md`, `toolkit-conventions.md`)
- `docs/reference/lorum_ipsum_dashboard.html` — Dave's prototype, the sheet UX
  baseline and the import contract's reference export
