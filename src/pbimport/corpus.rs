//! The shared items-corpus reader (gh#62): one module, one SQL statement,
//! every consumer. The bootstrap's `item_traits`/`item_bulk` maps and the
//! strike trait map read the SAME rows through the SAME query, so the
//! stored document shape cannot drift between them (the defect this epic
//! kills: the bootstrap queried `data->'system'` while the importer stores
//! the pack document at `data->'upstream'->'system'` — NULL on every real
//! row).

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
