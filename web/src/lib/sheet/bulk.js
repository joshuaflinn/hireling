// Inventory rollup math (E6 design §5) — pure, tested.
// Bulk lives in tenths everywhere: L=1, one Bulk=10, "—"/null=0. Container
// groups sum member item bulk × qty; containers flagged extradimensional
// are excluded from every total (their contents never count toward Bulk);
// the character total excludes extradimensional contents too.

/**
 * @typedef {Object} InventoryItemView
 * @property {string} name
 * @property {number} qty effective quantity (base + live delta)
 * @property {number|null} bulkTenths null = corpus gap → rendered "—"
 * @property {string} bulkText
 * @property {string[]} traits corpus trait chips
 * @property {boolean} invested
 */

/**
 * @typedef {Object} InventoryGroupView
 * @property {string | null} name null = the top level
 * @property {boolean} extradimensional
 * @property {InventoryItemView[]} items
 * @property {number | null} bulkTenths null for extradimensional groups
 * @property {string} bulkText
 */

/**
 * @param {*} baseSheet the normalized sheet (equipment + containers +
 * weapons + armor)
 * @param {Record<string, number | null>} itemBulk the bootstrap bulk map
 * @param {Record<string, string[]>} itemTraits the bootstrap trait map
 * @param {(name: string) => number} qtyOf effective quantity per item name
 * @returns {{ groups: InventoryGroupView[], totalTenths: number, totalText: string }}
 */
export function inventoryView(baseSheet, itemBulk, itemTraits, qtyOf) {
  const extradimensional = new Set(
    (baseSheet.containers ?? [])
      .filter((/** @type {any} */ container) => container.extradimensional)
      .map((/** @type {any} */ container) => container.name),
  );

  /** @type {Map<string | null, InventoryItemView[]>} */
  const grouped = new Map();
  for (const item of baseSheet.equipment ?? []) {
    const tenths = itemBulk[item.name] ?? null;
    const view = {
      name: item.name,
      qty: qtyOf(item.name),
      bulkTenths: tenths,
      bulkText: tenthsText(tenths),
      traits: itemTraits[item.name] ?? [],
      invested: Boolean(item.invested),
    };
    const key = item.container ?? null;
    if (!grouped.has(key)) grouped.set(key, []);
    /** @type {InventoryItemView[]} */ (grouped.get(key)).push(view);
  }

  const keys = [...grouped.keys()].sort((a, b) => {
    if (a === null) return -1; // top level first
    if (b === null) return 1;
    return a.localeCompare(b);
  });

  let total = 0;
  const groups = keys.map((name) => {
    const items = /** @type {InventoryItemView[]} */ (grouped.get(name));
    const extra = name !== null && extradimensional.has(name);
    const sum = extra
      ? null
      : items.reduce(
          /** @type {(acc: number, item: InventoryItemView) => number} */
          (acc, item) => acc + (item.bulkTenths ?? 0) * item.qty,
          0,
        );
    if (sum !== null) total += sum;
    return {
      name,
      extradimensional: extra,
      items,
      bulkTenths: sum,
      bulkText: extra ? 'excluded' : tenthsText(sum ?? 0),
    };
  });

  // Carried and worn both count (PF2e Bulk), every row at `(listed) ×
  // qty`, gaps degrading to 0 — never blocking, never invented:
  //
  // - `weapons` join at their listed Bulk — the listed value IS the held
  //   value (Player Core pg. 275, Weapons → Bulk).
  // - Armor (Player Core pg. 271, Armor Statistics → Bulk): a worn suit
  //   joins at its listed Bulk (the listed value IS the worn value); a
  //   carried suit usually joins at listed + 1 Bulk, and a suit of listed
  //   L counts 1 Bulk total — the +1 is a whole Bulk, not a light item
  //   stacked on L. A carried suit with an unresolved listed value stays
  //   a gap: unknown + 1 is unknown, so no floor is invented for it.
  // - Shield rows join at their listed Bulk held OR stowed: Pathbuilder
  //   files shields under `build.armor` with `prof: "shield"`, and on a
  //   shield row `worn` means held-in-hand (evidence: the second real
  //   export pinned in web/tests/data/pb_export_taraan_skyseeker.json —
  //   a held Caster's Targe rides build.armor, worn: true — matching the
  //   Pathmuncher importer's `carryType: "held"` mapping for worn
  //   shields). A shield's listed Bulk is its weight in hand (Player
  //   Core pg. 275, Shields → Bulk); the carried-suit +1 is armor
  //   statistics for suits, not shields.
  //
  // A row without `prof` reads as a suit (both real export samples carry
  // `prof` on every armor row), and a row without `worn` reads as carried
  // (`!item.worn` is true for `undefined`; both real export samples carry
  // the field, so this is a defensive default, not a live path). Carried
  // is the safe direction: the carried rate always charges more than the
  // worn rate, never less, so a dropped flag can only add Bulk, never
  // hide it. These rows render in the strikes/AC panes, not the inventory
  // list, so they add to the total without adding list rows.
  const weaponsTenths = (baseSheet.weapons ?? []).reduce(
    /** @type {(acc: number, item: any) => number} */
    (acc, item) => acc + (itemBulk[item.name] ?? 0) * (item.qty ?? 1),
    0,
  );
  const armorTenths = (baseSheet.armor ?? []).reduce(
    /** @type {(acc: number, item: any) => number} */
    (acc, item) => {
      const listed = itemBulk[item.name];
      if (listed === null || listed === undefined) return acc; // corpus gap → 0
      const carriedSuit = !item.worn && item.prof !== 'shield';
      // L (1 tenth) carries as 1 Bulk (10); a listed ≥ 1 Bulk carries as
      // listed + 1 Bulk (Player Core pg. 271, Armor Statistics → Bulk).
      const tenths = carriedSuit ? (listed < 10 ? 10 : listed + 10) : listed;
      return acc + tenths * (item.qty ?? 1);
    },
    0,
  );
  total += weaponsTenths + armorTenths;

  return { groups, totalTenths: total, totalText: tenthsText(total) };
}

/**
 * Bulk text from tenths — the shared formatter, with null (a corpus gap)
 * rendered "—" and zero "negligible" (prototype `bulkTxt` semantics).
 * @param {number | null} tenths
 * @returns {string}
 */
export function tenthsText(tenths) {
  if (tenths === null || tenths === undefined) return '—';
  const bulk = Math.floor(tenths / 10);
  const light = tenths % 10;
  if (bulk && light) return `${bulk} Bulk + ${light} L`;
  if (bulk) return `${bulk} Bulk`;
  if (light) return `${light} L`;
  return 'negligible';
}
