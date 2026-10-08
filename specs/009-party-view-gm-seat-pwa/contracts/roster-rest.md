# Contract: Party Roster Read (E10)

**Status**: authored 2026-10-08 (design D1); `assumed-until-probed` does not
apply — the payload shape is *captured* from the live
`GET /api/characters/me` implementation (`src/pbimport/handlers.rs::load_me`,
merged E5), and this contract pins the roster wrapper around it. If `me`'s
shape changes by PR, this document changes in the same PR.

## Endpoint

`GET /api/party/roster` — inside E3's session-protected nest (session
cookie; no session → 401 before the handler; `gm_read_only` middleware is
irrelevant — this is a read).

## Response

`200 OK` — `application/json`:

```jsonc
{
  "party_id": 1,
  "you": { "sub": "…", "role": "player" },   // role ∈ "player" | "gm" — mirrors /api/me
  "characters": [ CharacterPayload… ]        // party-scoped, ORDER BY characters.id
}
```

`CharacterPayload` = the current `GET /api/characters/me` 200 body
(`data-model.md` §1 captures it field-for-field). The roster introduces no
character fields.

Empty party is `200` with `characters: []` — never 204 (the wrapper's
`party_id`/`you` are meaningful even with no characters; 204 is `me`'s
"caller has no character" answer and would be wrong here).

## Errors

| status | when |
|---|---|
| `401` | no session (middleware, existing behavior) |
| `409` | resolution rule cannot resolve exactly one party (zero parties, or >1 and caller owns none — `data-model.md` §2) |
| `500` | database failure (logged with request id; empty body per house error style) |

## Consumer obligations (the drill-in contract)

- `SheetView` consumes a `CharacterPayload` unchanged; the party view passes
  the array element for the tapped card — no client-side reshaping.
- The client MUST treat the payload as bootstrap-only: live values arrive
  via the WS snapshot/diffs keyed by the same `character_id`; versions in
  slot/inventory rows exist for that merge.
- `you.role` is the UI's `editable` input for drill-ins (owner-sub match AND
  role `player`), mirroring E3's server rule — it is a rendering hint, never
  an enforcement claim.
