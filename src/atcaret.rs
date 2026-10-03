//! What the list at the text cursor offers for a search (2.4 A4): special
//! characters (every one Unicode names, see [`crate::chars`]) and the
//! typist's own snippets, ranked together by [`crate::find`]. App commands
//! and equations join in later steps.
//!
//! No OS calls; the search text is the caller's to wipe.

use crate::chars;
use crate::find::Query;
use crate::snippets::Snippet;

/// What a row is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Character,
    Snippet,
}

/// One row of the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub kind: Kind,
    /// Big, at the left: the character, or the snippet's trigger.
    pub glyph: String,
    /// What it is: the character's name, or the start of the snippet.
    pub label: String,
    /// Dim, at the right: `U+2192`, or empty.
    pub detail: String,
    /// What picking it types.
    pub text: String,
    score: u32,
}

/// Rows shown at most.
pub const MAX_ROWS: usize = 8;

/// The rows for `typed`, best first. `thai`: the interface language, for
/// the characters' names.
pub fn search(typed: &str, snippets: &[Snippet], thai: bool) -> Vec<Row> {
    let query = Query::new(typed);
    if query.is_empty() {
        return Vec::new();
    }
    let mut rows: Vec<Row> = snippets
        .iter()
        // A misspelling of the typist's own is a fix, not something to type.
        .filter(|s| s.scope != crate::snippets::Scope::Typo)
        .filter_map(|s| {
            let score = query.score_any([s.trigger.as_str(), s.text.as_str()])?;
            let mut label: String = s.text.chars().take(40).collect();
            if s.text.chars().count() > 40 {
                label.push('…');
            }
            Some(Row {
                kind: Kind::Snippet,
                glyph: s.trigger.clone(),
                label: label.replace(['\r', '\n'], " "),
                detail: String::new(),
                text: s.text.clone(),
                // The typist's own come first among equals.
                score: score + 1,
            })
        })
        .collect();
    rows.extend(
        chars::search(&query, typed, MAX_ROWS)
            .into_iter()
            .map(|(score, e)| Row {
                kind: Kind::Character,
                glyph: e.text.to_string(),
                label: e.label(thai).to_string(),
                detail: e.code(),
                text: e.text.to_string(),
                score,
            }),
    );
    // Stable: equal scores keep snippets first, then code point order.
    rows.sort_by_key(|r| std::cmp::Reverse(r.score));
    rows.truncate(MAX_ROWS);
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snippets::Scope;

    fn sig() -> Vec<Snippet> {
        vec![Snippet {
            trigger: ";sig".into(),
            text: "Best regards,\nAffan".into(),
            scope: Scope::Either,
        }]
    }

    #[test]
    fn characters_and_snippets_together() {
        let rows = search("degree", &sig(), false);
        assert_eq!(rows[0].glyph, "°");
        assert_eq!(rows[0].detail, "U+00B0");
        assert_eq!(rows[0].kind, Kind::Character);
        let rows = search("regards", &sig(), false);
        assert_eq!(rows[0].kind, Kind::Snippet);
        assert_eq!(rows[0].label, "Best regards, Affan");
        assert_eq!(rows[0].text, "Best regards,\nAffan");
    }

    #[test]
    fn thai_names_in_the_thai_interface() {
        let rows = search("ยูโร", &[], true);
        let euro = rows.iter().find(|r| r.glyph == "€").expect("€");
        assert!(euro.label.contains("ยูโร"), "{}", euro.label);
        assert!(search("", &sig(), true).is_empty());
        assert!(search("anything at all", &[], false).len() <= MAX_ROWS);
    }
}
