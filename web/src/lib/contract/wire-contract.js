/* Wire contract gate — BROWSER-SAFE HALF. The pin's DATA lives here, inside the
 * `src/**` check graph, because `web/tsconfig.json` includes `src/**` only and
 * svelte-check's language service reads no config below `web/`: a helper that
 * imports `node:fs` cannot be reached from production code at all. The fixture
 * stays the source of truth — `web/tests/helpers/wire-contract.js` asserts these
 * literals EQUAL `field-targets.json`, on a production path — and it re-exports
 * the two assertions below, so tests and production run one gate, not two. */

/** The pinned set of legal `value.op` literals for a `stash` target, as the
 *  machine-readable half of wire-protocol §3. Declared inline rather than read
 *  off the fixture so this module can sit inside the `src/**` check graph
 *  (`web/tsconfig.json`'s include is `src/**` only, and `svelte-check`'s
 *  language service reads no config below `web/`, so a `node:fs` import anywhere
 *  under `src/` is unresolvable — see the header of
 *  `web/tests/helpers/wire-contract.js`). The fixture remains the source of
 *  truth: the assertion that this list equals `stash_ops` lives there, on a
 *  production path, so a fixture that gains or drops an op reddens a suite.
 *  @type {ReadonlySet<string>} */
export const stashOps = new Set(["meta", "qty", "sell", "transfer"]);

/** Assert a stash frame's `value.op` against the pinned op set — the same
 * gate `assertEffectOp` is, one kind over. `qty` is additive (§4: no version
 * predicate) and `meta` is absolute under CAS; both are legal today, and
 * anything else the client builds is a frame the server refuses.
 * @param {{op: string}} value the write frame's `value` */
export function assertStashOp(value) {
  if (!value || typeof value !== 'object') {
    throw new Error('a stash write value is an object');
  }
  if (!stashOps.has(value.op)) {
    throw new Error(
      `unknown stash op "${value.op}" — the pinned contract accepts: ${[...stashOps].join(', ')}`,
    );
  }
}

/** The pinned target kinds and their sibling field sets, straight from the
 *  fixture. Same duplication rule and same proof as `stashOps` above.
 *  @type {Readonly<Record<string, {fields: string[]}>>} */
export const targetFields = {
  "vitals": {
    "fields": [
      "character_id",
      "field"
    ]
  },
  "slot": {
    "fields": [
      "character_id",
      "caster_key",
      "rank",
      "slot_index"
    ]
  },
  "inv": {
    "fields": [
      "character_id",
      "item_name"
    ]
  },
  "effect": {
    "fields": [
      "effect_id"
    ]
  },
  "effect_new": {
    "fields": [
      "party_id"
    ]
  },
  "stash_new": {
    "fields": [
      "party_id"
    ]
  },
  "stash": {
    "fields": [
      "party_id",
      "item_name"
    ]
  },
  "bank": {
    "fields": [
      "party_id"
    ]
  },
  "party": {
    "fields": [
      "party_id",
      "field"
    ]
  },
  "claim": {
    "fields": [
      "claim_id"
    ]
  }
};

/** The pinned legal `vitals` `field` values (gh#91). @type {ReadonlySet<string>} */
export const vitalsFields = new Set(["daily", "focus_current", "hero_points", "hp", "level_adjust", "money", "shield_raised", "temp_hp"]);

/** Assert one write frame's `target` against the pinned contract: the `kind`
 * must be one the server decodes, and the key set must match the contract
 * exactly — a missing or an extra sibling field both fail.
 *
 * @param {{kind: string, field?: string}} target the frame's `target` value.
 *   `field` is a sibling on the `party` and `vitals` targets only, and the
 *   key-set check below is what proves a kind that must NOT carry it does not —
 *   this optional member exists so the vitals branch can read the one the
 *   contract allows, not to widen what any caller may send. */
export function assertWriteTarget(target) {
  if (!target || typeof target !== 'object') {
    throw new Error('a write target is an object');
  }
  const entry = targetFields[target.kind];
  if (!entry) {
    throw new Error(
      `unknown write target kind "${target.kind}" — the pinned contract knows: ${Object.keys(targetFields).join(', ')}`,
    );
  }
  const siblings = Object.keys(target)
    .filter((key) => key !== 'kind')
    .sort(); // the tag is checked above, not a sibling
  if (siblings.join() !== [...entry.fields].sort().join()) {
    throw new Error(
      `the "${target.kind}" target's field set must match the pinned contract exactly`,
    );
  }
  // `field` is optional on the param type because it is a sibling on two kinds
  // only; the key-set check above has already proven a `vitals` target carries
  // exactly one, so `undefined` here is a contract violation, not a legal shape —
  // which is why it falls through to the refusal below rather than needing a
  // separate branch.
  if (target.kind === 'vitals' && !vitalsFields.has(/** @type {string} */ (target.field))) {
    throw new Error(
      `unknown vitals field "${target.field}" — the pinned contract accepts: ${[...vitalsFields].join(', ')}`,
    );
  }
}
