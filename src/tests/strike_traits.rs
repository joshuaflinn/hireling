//! Strike traits ride the corpus on the live recompute path (gh#62 design
//! D9): a member seeded through the real E5 pipeline, a corpus seeded
//! through the real import pipeline, then [`recompute_character`] — the
//! production path the sheet renders from. Unit fixtures with hand-built
//! maps prove `extract`; this proves the LOADER is on the path — a
//! bootstrap-only fix or an unwired parameter fails here, not in a unit.

use crate::engine_host::recompute::recompute_character;
use crate::pbimport::payload::character_payload;
use crate::pbimport::{model, transform};
use crate::testing;

/// The prototype's own export, verbatim — the sheet the member carries,
/// with a Shortsword and a Longspear injected into `build.weapons`.
const REFERENCE: &str = include_str!("../../tests/data/pb_export_reference.json");

/// Seed a member whose sheet carries the reference's weapons PLUS a
/// Shortsword (corpus-backed, agile+finesse) and a Longspear (no corpus
/// row) — the miss arm of the degradation contract.
async fn seed_member_with_injected_weapons(pool: &sqlx::PgPool, sub: &str) -> i64 {
    let export = model::parse_and_validate(REFERENCE).expect("reference export is valid");
    let (mut sheet, skips) = transform::transform(&export);
    assert!(
        skips.sections.is_empty(),
        "the reference export transforms without skips"
    );
    let weapons = sheet.weapons.get_or_insert_with(|| serde_json::json!([]));
    let weapons_array = weapons
        .as_array_mut()
        .expect("the reference weapons section is an array");
    weapons_array.push(serde_json::json!({
        "name": "Shortsword", "die": "d6", "attack": 5, "damageBonus": 0
    }));
    weapons_array.push(serde_json::json!({
        "name": "Longspear", "die": "d8", "attack": 5, "damageBonus": 0
    }));
    let sheet_json = serde_json::to_value(&sheet).expect("sheet serializes");
    testing::seed_account(pool, sub, "player").await;
    let party_id: i64 = sqlx::query_scalar("SELECT id FROM parties ORDER BY id LIMIT 1")
        .fetch_one(pool)
        .await
        .expect("POC party");
    let character_id: i64 = sqlx::query_scalar(
        "INSERT INTO characters (party_id, owner_sub, payload_raw, base_sheet) \
         VALUES ($1, $2, '{}', $3) RETURNING id",
    )
    .bind(party_id)
    .bind(sub)
    .bind(sheet_json)
    .fetch_one(pool)
    .await
    .expect("seed member");
    sqlx::query("INSERT INTO character_vitals (character_id) VALUES ($1)")
        .bind(character_id)
        .execute(pool)
        .await
        .expect("seed vitals");
    character_id
}

fn wire_strike<'a>(
    output: &'a hireling_engine::model::EngineOutput,
    key: &str,
) -> &'a hireling_engine::model::StrikeOutput {
    output
        .derived
        .strikes
        .iter()
        .find(|strike| strike.key == key)
        .unwrap_or_else(|| panic!("strike {key} on the wire"))
}

/// The loader feeds `extract` on the production path: the Shortsword
/// renders its corpus traits in display form with the agile MAP step and
/// a finesse attack (Dex +1 beats Str −1); the Staff's row arrives from
/// the corpus with identical rendered values to the golden's pins; the
/// Longspear — no corpus row — degrades to empty traits and −5/−10.
#[tokio::test]
async fn recompute_renders_strike_traits_from_the_corpus() {
    let Some(pool) = testing::test_pool().await else {
        eprintln!("skipping: no test database");
        return;
    };
    let character_id = seed_member_with_injected_weapons(&pool, "strike-traits-player").await;

    // The corpus, through the REAL import pipeline: the Shortsword is
    // agile+finesse in the pack's key form, the Staff carries its row.
    // The Longspear is deliberately absent.
    let report = crate::import::import_from_bytes(
        &pool,
        "pf2e-8.5.1",
        &testing::corpus_seed_zip(&[
            ("Shortsword", 0.1, &["agile", "finesse", "versatile-s"]),
            ("Staff", 1.0, &["monk", "two-hand-d8"]),
        ]),
        include_str!("../../data/seed/condition-tiers.json"),
    )
    .await
    .expect("corpus seed import runs");
    assert!(report.success, "the corpus seed import succeeds");

    let output = recompute_character(&pool, character_id)
        .await
        .expect("recompute succeeds");

    let shortsword = wire_strike(&output, "Shortsword");
    assert_eq!(
        shortsword.traits,
        vec!["Agile", "Finesse", "Versatile S"],
        "corpus keys render in display form"
    );
    assert_eq!(shortsword.map, 4, "Agile: −4/−8 on the live path");
    assert_eq!(
        shortsword.attack.total, 1,
        "finesse: dex +1 on the attack, not str −1"
    );

    let staff = wire_strike(&output, "Staff");
    assert_eq!(
        staff.traits,
        vec!["Monk", "Two-Hand d8"],
        "the Staff's corpus row matches the golden's pinned values"
    );
    assert_eq!(staff.map, 5, "the Staff is not agile: −5/−10");

    let longspear = wire_strike(&output, "Longspear");
    assert_eq!(
        longspear.traits,
        Vec::<String>::new(),
        "a corpus miss degrades honestly — nothing invented"
    );
    assert_eq!(longspear.map, 5, "a miss is not agile: −5/−10");

    testing::drop_test_db(pool, "strike_traits").await;
}

/// The lane-collision regression (PR #159 review round 1): a custom-lane
/// row sharing a name with an imported row must not shadow it. The spec
/// (FR-1) sources strike traits from the IMPORTED corpus and calls a
/// custom-lane item a miss — so on a name collision the imported row is
/// the one every consumer reads. The old reader had no lane filter and no
/// ORDER BY: `weapon_trait_map` (map-extend, LAST row wins) and the
/// bootstrap (`find`, FIRST row wins) could resolve one name to OPPOSITE
/// rows — planner-order dependent, and the custom row reads back empty
/// (custom rows carry no upstream document), so the strike silently lost
/// its chips, its agile MAP step, and its finesse attack. Both production
/// consumers are asserted so ANY row order fails this test on the old
/// query: whichever way the heap lays the rows out, at least one consumer
/// disagrees with the imported row.
#[tokio::test]
async fn custom_same_name_row_cannot_shadow_the_imported_corpus() {
    let Some(pool) = testing::test_pool().await else {
        eprintln!("skipping: no test database");
        return;
    };
    let sub = "strike-lane-player";
    let character_id = seed_member_with_injected_weapons(&pool, sub).await;

    // The imported Shortsword, through the REAL import pipeline.
    let report = crate::import::import_from_bytes(
        &pool,
        "pf2e-8.5.1",
        &testing::corpus_seed_zip(&[("Shortsword", 0.1, &["agile", "finesse", "versatile-s"])]),
        include_str!("../../data/seed/condition-tiers.json"),
    )
    .await
    .expect("corpus seed import runs");
    assert!(report.success, "the corpus seed import succeeds");

    // A custom-lane row of the SAME name — the exact shape the shipped
    // custom create path writes (`custom.rs::custom_block`: no upstream
    // document, `created_by_sub` set, `source_id` null).
    sqlx::query(
        "INSERT INTO corpus_entries (kind, name, lane, data, created_by_sub) \
         VALUES ('item', 'Shortsword', 'custom', $1, $2)",
    )
    .bind(serde_json::json!({"custom": {"description": "a homemade blade"}}))
    .bind(sub)
    .execute(&pool)
    .await
    .expect("seed the custom-lane twin");

    // The strike side: recompute — the production path the sheet renders
    // from — must still read the IMPORTED row: chips, agile MAP, finesse.
    let output = recompute_character(&pool, character_id)
        .await
        .expect("recompute succeeds");
    let shortsword = wire_strike(&output, "Shortsword");
    assert_eq!(
        shortsword.traits,
        vec!["Agile", "Finesse", "Versatile S"],
        "the custom twin must not blank the imported traits"
    );
    assert_eq!(shortsword.map, 4, "the imported Agile row keeps −4/−8");
    assert_eq!(
        shortsword.attack.total, 1,
        "the imported Finesse row keeps the Dex attack"
    );

    // The bootstrap side: the party payload's chip and bulk maps must
    // agree with the strike side — one row per name, the imported one.
    let payload = character_payload(&pool, character_id)
        .await
        .expect("bootstrap payload resolves");
    assert_eq!(
        payload
            .get("item_traits")
            .and_then(|traits| traits.get("Shortsword")),
        Some(&serde_json::json!(["Agile", "Finesse", "Versatile S"])),
        "bootstrap chips read the same imported row the strike reads"
    );
    assert_eq!(
        payload
            .get("item_bulk")
            .and_then(|bulk| bulk.get("Shortsword")),
        Some(&serde_json::json!(1)),
        "bootstrap bulk reads the imported 0.1 → one tenth"
    );

    testing::drop_test_db(pool, "strike_traits").await;
}
