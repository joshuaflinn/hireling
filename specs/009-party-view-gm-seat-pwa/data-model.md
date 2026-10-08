# Data Model — E10 Party View, GM Seat, PWA

Entities E10 introduces or carries. Everything else (characters, vitals,
slots, inventory, effects, accounts, parties) is E2/E3's model, consumed
as-is — no migrations ship in this epic.

## 1. Roster response (`GET /api/party/roster`)

```jsonc
{
  "party_id": 1,
  "you": { "sub": "dev-sub-josh", "role": "player" },   // role: "player" | "gm"
  "characters": [ /* CharacterPayload[], one per party character, order by id */ ]
}
```

`CharacterPayload` is exactly `GET /api/characters/me`'s 200 body today
(captured from `src/pbimport/handlers.rs::load_me`, not re-invented):

```jsonc
{
  "id": 3,
  "name": "Flinn",          // identity.name off base_sheet (null-safe)
  "level": 5,               // identity.level
  "class": "Wizard",        // identity.class
  "owner": "dev-sub-josh",  // characters.owner_sub
  "base_sheet": { /* E5's stored export transform, verbatim */ },
  "vitals": { "hp": 42, "temp_hp": 0, "money_pp": 0, "money_gp": 50,
              "money_sp": 7, "money_cp": 3, "level_adjust": 0,
              "focus_current": 2, "hero_points": 1, "daily": {…} },
  "slots":   [ /* slot rows: caster_key, rank, slot_index, used, prepared, version */ ],
  "inventory": [ /* item rows: item_name, qty, version */ ]
}
```

Validation rules: handler-level only (it is a read of validated rows);
`identity.*` reads are null-safe with `null` (never absent keys) in the
summary; `characters` may be `[]` (the empty party is a valid state, not an
error).

State transitions: none — the response is a point-in-time bootstrap; all
liveness rides the WS (versions in `slots`/`inventory` rows exist for the
store's merge, mirroring the snapshot's).

## 2. Party resolution rule (server, `src/party/mod.rs`)

```
resolve_party(caller):
  if caller owns a character      → that character's party_id          (unique — one character per account at POC)
  else if parties count == 1      → that party
  else if parties count == 0      → 409 "no party exists"             (honest, not guessed)
  else                            → 409 "ambiguous party for caller"  (cannot happen at POC; the rule exists so it can't silently pick)
```

Caller classes: player with character (rule 1), player without (rule 2/3),
GM (rule 2/3). Authorization: any authenticated session (E3 `Read → Allow`);
the response never includes characters outside the resolved party (query is
party-scoped, not account-scoped).

## 3. Boot cache (client, `hireling:boot:{account_sub}`)

```jsonc
{
  "roster":   { /* the roster response above, verbatim */ },
  "snapshot": { "fields": [ /* wire-shaped snapshot from sync.snapshotForBoot() */ ] },
  "saved_at": 1760000000000   // diagnostics only; NEVER read for staleness decisions (client clocks untrusted — versions merge, not timestamps)
}
```

Rules: written per successful roster fetch + throttled (2 s trailing) on
store merges; keyed per account sub (the queue's isolation pattern);
`saved_at` is display/diagnostic only. Seeding uses the snapshot's per-field
versions through the existing strictly-newer merge — a stale cache can never
overwrite fresher wire state.

## 4. SW shell cache (client, Cache Storage)

- Cache name: `hireling-shell-{BUILD_ID}` where `BUILD_ID` is the content
  hash of the built bundle (emitted into `sw.js` by the vite plugin).
- Precache set: `index.html`, every emitted `/assets/*` file,
  `manifest.webmanifest`, the three icons.
- No request data, no API responses, no per-account data ever enters Cache
  Storage (that is the boot cache's job, §3).

## 5. Entities not modeled here (guarded)

Roster membership is derived (characters of the party), not a new table.
"Down"/"full" are render states of `hp` vs `render_base.hp_max`, not stored.
Portrait initial is `name[0]` uppercased at render, not stored. GM seat is
E3's `role`, not a party-membership row.
