@AGENTS.md
@docs/toolkit-conventions.md

## How work gets done here
Opus plans, cheaper tiers execute:

- This session does the reading, planning, verifying, and all writing Dave sees.
- Implementation, multi-file edits, scripts, build-guide drafting, and test runs go to
  `subagent_type: "implementer"` (Sonnet).
- Inventories, grep sweeps, format conversion, and field extraction go to
  `subagent_type: "scout"` (Haiku).
- Acceptance-criteria checks on a finished build go to `subagent_type: "verifier"` (Sonnet).
- Independent tasks get spawned in parallel in one message. Anything under roughly three
  tool calls stays in-session.
- Every task prompt carries absolute paths, the exact change wanted, the project
  constraints that matter, and what "done" looks like.
