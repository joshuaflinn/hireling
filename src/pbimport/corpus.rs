//! The shared items-corpus reader (gh#62): one module, one SQL statement,
//! every consumer. The bootstrap's `item_traits`/`item_bulk` maps and the
//! strike trait map read the SAME rows through the SAME query, so the
//! stored document shape cannot drift between them (the defect this epic
//! kills: the bootstrap queried the pack document at the document root
//! while the importer stores it under the `upstream` key — the read
//! returned NULL on every real row).

/// Display form for one corpus trait key (design D3): split on `-`, every
/// segment capitalized; the LAST segment joins with a space instead of a
/// hyphen when it is a die (`d8` — lowercase `d` kept), a bare number, or
/// a single letter (uppercased); otherwise it stays hyphen-joined. The
/// pinned mappings live in the tests below; the prototype's rendered
/// chips (`Two-Hand d8`) are display truth.
#[must_use]
pub fn display_trait(corpus_key: &str) -> String {
    let segments: Vec<&str> = corpus_key.split('-').filter(|s| !s.is_empty()).collect();
    let Some((last, head)) = segments.split_last() else {
        return String::new();
    };
    let is_die = |segment: &str| {
        segment
            .strip_prefix('d')
            .is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
    };
    let is_number = last.bytes().all(|b| b.is_ascii_digit());
    let is_single_letter =
        last.len() == 1 && last.chars().next().is_some_and(|c| c.is_ascii_alphabetic());
    // Die segments keep the lowercase `d` (`d8` never `D8` — the
    // prototype renders "Two-Hand d8"); a lone letter renders uppercase;
    // everything else capitalizes its first character and rides verbatim.
    let render = |segment: &str| -> String {
        let mut chars = segment.chars();
        let Some(first) = chars.next() else {
            return String::new();
        };
        if is_die(segment) || (segment.len() > 1 && segment.bytes().all(|b| b.is_ascii_digit())) {
            segment.to_owned()
        } else if segment.len() == 1 && first.is_ascii_alphabetic() {
            segment.to_ascii_uppercase()
        } else {
            first.to_uppercase().collect::<String>() + chars.as_str()
        }
    };
    let space_join = is_die(last) || is_number || is_single_letter;
    let joiner = if space_join { " " } else { "-" };
    let tail = render(last);
    if head.is_empty() {
        return tail;
    }
    let head_rendered: String = head
        .iter()
        .map(|segment| render(segment))
        .collect::<Vec<String>>()
        .join("-");
    format!("{head_rendered}{joiner}{tail}")
}

/// One resolved items-corpus row (design D2): the lowercased name (the
/// case-insensitive join key every consumer already uses), the raw bulk
/// decimal, and the trait keys in DISPLAY form — `display_trait` applied
/// here, at the single boundary every map reads through.
pub struct ItemCorpusRow {
    pub name_lower: String,
    pub bulk_raw: Option<f64>,
    pub traits: Vec<String>,
}

/// Every requested name's corpus row, one query (design D2). The importer
/// stores the pack document under `data.upstream.system`, so THAT is the
/// shape this reads; traits come back in display form.
///
/// Exactly ONE row per name — the schema constrains only
/// `(kind, source_id)`, so a custom-lane row may share a name with an
/// imported one (PR #159 review round 1). Resolution is deterministic and
/// lane-precedenced: the IMPORTED row wins a collision — E10 FR-6
/// (`specs/010-rules-tooltips-custom-content/spec.md`) keeps engine math
/// out of custom rows entirely, and the engine-output contract
/// (`specs/008-buff-effect-engine/contracts/engine-output.md`) sources
/// strike traits from the imported corpus and calls a custom-lane item a
/// miss, so a custom twin must never blank a pack weapon's traits. The
/// `id` tail-break keeps the order total (a hypothetical duplicate inside
/// one lane still resolves stably, not planner-ordered). Consumers disagree
/// about multi-row results by construction otherwise — `weapon_trait_map`
/// extends (last wins) while the bootstrap `find`s (first wins) — which is
/// exactly why the collapse happens HERE, once, in the shared statement.
///
/// # Errors
///
/// Propagates sqlx failures.
pub async fn item_rows(
    pool: &sqlx::PgPool,
    names: &[String],
) -> Result<Vec<ItemCorpusRow>, sqlx::Error> {
    let rows: Vec<(String, Option<String>, Option<serde_json::Value>)> = sqlx::query_as(
        "SELECT DISTINCT ON (lower(name)) lower(name), \
                data->'upstream'->'system'->'bulk'->>'value', \
                data->'upstream'->'system'->'traits'->'value' \
         FROM corpus_entries \
         WHERE kind = 'item' AND lower(name) = ANY($1) \
         ORDER BY lower(name), (lane = 'imported') DESC, id",
    )
    .bind(
        names
            .iter()
            .map(|name| name.to_lowercase())
            .collect::<Vec<_>>(),
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(name_lower, raw_bulk, raw_traits)| ItemCorpusRow {
            bulk_raw: raw_bulk.as_ref().and_then(|text| text.parse::<f64>().ok()),
            traits: raw_traits
                .and_then(|value| value.as_array().cloned())
                .unwrap_or_default()
                .iter()
                .filter_map(|value| value.as_str())
                .map(display_trait)
                .collect(),
            name_lower,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::display_trait;

    #[test]
    fn pinned_corpus_keys_render_in_display_form() {
        assert_eq!(display_trait("agile"), "Agile");
        assert_eq!(display_trait("two-hand-d8"), "Two-Hand d8");
        assert_eq!(display_trait("deadly-d8"), "Deadly d8");
        assert_eq!(display_trait("thrown-10"), "Thrown 10");
        assert_eq!(display_trait("versatile-s"), "Versatile S");
        assert_eq!(display_trait("jousting-d6"), "Jousting d6");
    }

    #[test]
    fn already_display_input_passes_through() {
        assert_eq!(display_trait("Agile"), "Agile");
        assert_eq!(display_trait("Two-Hand d8"), "Two-Hand d8");
    }

    #[test]
    fn the_die_keeps_its_lowercase_d() {
        assert_eq!(display_trait("d8"), "d8");
        assert_eq!(display_trait("d6"), "d6");
    }
}
