# Requirements Sweep — E9

Every requirement → the production-path test that proves it (AGENTS.md
rule: the test exercises the path that owns the behavior, not the module
underneath it). The PR body reproduces this table with real test names.

| Req | Requirement (spec) | Production-path proof | Task |
|---|---|---|---|
| FR-1 | Condition tooltips on every name surface | `ConditionTip` suite (hover/focus/pin/Esc, fireEvent) + `EffectsStrip`/`Provenance` render-path tests with a matching and a non-matching fixture | T3/T4 |
| FR-2 | 42-condition curated coverage | Coverage test: every seed key joins a pinned corpus fixture name; renamed fixture proves the join | T2 |
| FR-3 | Inert rendering everywhere | Hostile matrix unit suite **and** the same fixtures through the mounted tip/about (component is the path); `{@html}` boundary grep | T1 + all |
| FR-4 | Add-custom at 3 points of use, caps, immediate surface, party-wide picker, badge | Router tests (201/400 per cap) + form tests (fireEvent input/click, inline errors, optimistic render) + picker/composer listing tests | T5/T7/T8/T9 |
| FR-5 | Creator-owned writes; GM read-only; audited refusals | Router tests asserting **persisted `audit_events` rows** for GM/other/lane PATCH attempts | T5/T6 |
| FR-6 | Custom = display/tracking (no math) | Apply through picker issues existing corpus create; chip renders `tracked`; zero modifiers asserted; no engine file touched | T9 (+T5) |
| FR-7 | Picker surface over E8 REST | `ConditionPicker` suite: badges per tier/lane/valued fixtures, valued input, apply call shape, owner gate | T9 |
| FR-8 | Importer never touches custom rows | Integration: real importer re-run with custom rows present; byte-identical after | T6 |
| FR-9 | NOTICE lane + in-app about view | About renders a known NOTICE line through the mounted component (inert fixture); `license-verdict` green | T10 |
| FR-10 | Tooltips offline | Seed is a build-time module (no runtime fetch — by construction); SW contract unchanged | T2/T11 |

Reachability sweep (PR-time): grep every new export for a caller;
findings confined to the defining module = unbuilt requirement.
