//! What the list at the text cursor offers for a search (2.4 A4): special
//! characters (every one Unicode names, see [`crate::chars`]) and the
//! typist's own snippets, ranked together by [`crate::find`]; LaTeX typed
//! into it (`\frac{1}{2}`, `x^2`) shows what it writes as Unicode first
//! (2.4 D3), and `\al` lists the commands that start so. App commands join
//! in a later step: the app's own commands (its menus, then the bundled
//! shortcut table) are rows too, and picking one presses its shortcut
//! (2.4 E).
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
    /// An app command: `label` is its name, `text` (and `detail`) its keys.
    Command,
    /// LaTeX written as Unicode. With no `text`, `label` says why it cannot
    /// be: nothing to pick.
    Equation,
    /// Math put into the app by steps ([`crate::macros`] written in
    /// `text`): its own equation editor, or its superscript formatting.
    EquationIn,
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

impl Row {
    /// A character on its own (the grid of pinned and recent ones).
    pub fn character(text: &str) -> Row {
        Row {
            kind: Kind::Character,
            glyph: text.to_string(),
            label: String::new(),
            detail: String::new(),
            text: text.to_string(),
            score: 0,
        }
    }
}

/// A command of the app in front: its name (in the interface language) and
/// the keys that run it (`Ctrl+H`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    pub name: String,
    pub keys: String,
    /// Its name in the other language, when known (the bundled table has
    /// both): `close` finds ปิดหน้าต่าง in the Thai interface.
    pub other: String,
}

/// Whether running `cmd` from a list could lose work (closing a window or a
/// tab, deleting, quitting): it then takes Enter twice.
pub fn needs_confirming(cmd: &Command) -> bool {
    let keys = cmd.keys.to_ascii_lowercase();
    let name = cmd.name.to_lowercase();
    let closes = matches!(
        keys.as_str(),
        "alt+f4" | "ctrl+w" | "ctrl+shift+w" | "ctrl+f4" | "shift+delete" | "shift+del"
    );
    let says_so = [
        "delete",
        "exit",
        "quit",
        "close",
        "remove",
        "ลบ",
        "ปิด",
        "ออกจาก",
    ]
    .iter()
    .any(|w| name.contains(w));
    closes || says_so
}

/// How far an app command ranks ahead of a character matching as well.
const COMMAND_BONUS: u32 = 200;

/// Rows shown at most.
pub const MAX_ROWS: usize = 8;

/// The rows for `typed`, best first. `thai`: the interface language, for
/// the characters' names.
pub fn search(typed: &str, snippets: &[Snippet], commands: &[Command], thai: bool) -> Vec<Row> {
    let query = Query::new(typed);
    if query.is_empty() {
        return Vec::new();
    }
    let mut rows = equation(typed, thai);
    // `\al`: the commands that start so, shortest first.
    if let Some(prefix) = typed.strip_prefix('\\') {
        if !prefix.is_empty() && prefix.chars().all(|c| c.is_ascii_alphabetic()) {
            let mut names: Vec<_> = crate::latex::names()
                .filter(|(n, _)| n.starts_with(prefix))
                .collect();
            names.sort_by_key(|(n, _)| n.len());
            rows.extend(names.into_iter().take(MAX_ROWS).map(|(n, s)| Row {
                kind: Kind::Character,
                glyph: s.to_string(),
                label: format!("\\{n}"),
                detail: String::new(),
                text: s.to_string(),
                score: u32::MAX - 1 - n.len() as u32,
            }));
        }
    }
    rows.extend(commands.iter().filter_map(|c| {
        let score = query.score_any([c.name.as_str(), c.other.as_str(), c.keys.as_str()])?;
        Some(Row {
            kind: Kind::Command,
            glyph: String::new(),
            label: c.name.clone(),
            detail: c.keys.clone(),
            text: c.keys.clone(),
            // The app's own commands before characters that match as well:
            // `close` is the window before it is a bracket.
            score: score + COMMAND_BONUS,
        })
    }));
    rows.extend(
        snippets
            .iter()
            // A misspelling of the typist's own is a fix, not something to type.
            .filter(|s| s.scope != crate::snippets::Scope::Typo)
            .filter_map(|s| {
                let score = query.score_any([s.trigger.as_str(), s.text.as_str()])?;
                // A macro says what it does; a snippet shows its text.
                let shown = match crate::macros::steps(&s.text) {
                    Some(Ok(steps)) => crate::macros::summary(&steps, thai),
                    _ => s.text.clone(),
                };
                let mut label: String = shown.chars().take(40).collect();
                if shown.chars().count() > 40 {
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
            }),
    );
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

/// The LaTeX row: what `typed` writes, or why it cannot, when it is LaTeX
/// at all. A lone command being typed (`\al`) is left to the command rows.
fn equation(typed: &str, thai: bool) -> Vec<Row> {
    // Written short (`x^(3/2) + (2x)/8 + pi`), as Word's equation editor
    // reads it: no backslash or brace (those are LaTeX).
    if !typed.contains(['\\', '{']) {
        if let Some(m) = crate::mathsimple::Math::read(typed) {
            let text = m.unicode();
            if text != typed.trim() {
                return vec![Row {
                    kind: Kind::Equation,
                    glyph: String::new(),
                    label: text.clone(),
                    detail: if thai { "คณิต" } else { "Math" }.into(),
                    text,
                    score: u32::MAX,
                }];
            }
        }
    }
    if !crate::latex::looks_like_latex(typed) {
        // Said in words: "x squared plus 1", "รากที่สองของ x".
        return crate::naturalmath::to_unicode(typed)
            .map(|text| Row {
                kind: Kind::Equation,
                glyph: String::new(),
                label: text.clone(),
                detail: if thai { "คณิต" } else { "Math" }.into(),
                text,
                score: u32::MAX,
            })
            .into_iter()
            .collect();
    }
    let lone = typed
        .strip_prefix('\\')
        .is_some_and(|p| p.chars().all(|c| c.is_ascii_alphabetic()));
    let (label, text) = match crate::latex::to_unicode(typed) {
        Ok(text) if text != typed => (text.clone(), text),
        Ok(_) => return Vec::new(),
        Err(_) if lone => return Vec::new(),
        Err(e) => (e.describe(thai), String::new()),
    };
    vec![Row {
        kind: Kind::Equation,
        glyph: String::new(),
        label,
        detail: "LaTeX".into(),
        text,
        score: u32::MAX,
    }]
}

/// For an app with an equation editor and superscript formatting (Word,
/// PowerPoint, OneNote): math written short put in as a real equation
/// (Alt+=, then the short form, which the app builds up), or as text with
/// superscripts and subscripts — after the Unicode row, or first when
/// Unicode cannot write all of it.
pub fn add_app_math(rows: &mut Vec<Row>, typed: &str, thai: bool) {
    if typed.contains(['\\', '{']) {
        return;
    }
    let Some(m) = crate::mathsimple::Math::read(typed) else {
        return;
    };
    let preview = m.unicode();
    let equation = Row {
        kind: Kind::EquationIn,
        glyph: String::new(),
        label: preview.clone(),
        detail: if thai {
            "สมการ (Alt+=)"
        } else {
            "Equation (Alt+=)"
        }
        .into(),
        // A space after builds up the last part too.
        text: format!("{{press Alt+=}}{{wait 400}}{} ", m.unicode_math()),
        score: u32::MAX,
    };
    let mut steps = String::new();
    for piece in m.formatted() {
        use crate::mathsimple::Piece;
        match piece {
            Piece::Text(t) => steps.push_str(&t),
            Piece::Sup(t) => {
                steps.push_str(&format!("{{press Ctrl+Shift+=}}{t}{{press Ctrl+Shift+=}}"))
            }
            Piece::Sub(t) => steps.push_str(&format!("{{press Ctrl+=}}{t}{{press Ctrl+=}}")),
        }
    }
    let formatted = Row {
        kind: Kind::EquationIn,
        glyph: String::new(),
        label: preview,
        detail: if thai {
            "ตัวยก/ตัวห้อย"
        } else {
            "Super/subscript"
        }
        .into(),
        text: steps,
        score: u32::MAX,
    };
    let at = if m.fully_unicode() && rows.first().is_some_and(|r| r.kind == Kind::Equation) {
        1
    } else {
        0
    };
    rows.insert(at, formatted);
    rows.insert(at, equation);
    rows.truncate(MAX_ROWS);
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
    fn math_written_short() {
        let typed = "x^2 + (2x)/8 + x/2 +x^(3/2) +pi + sigma + Sigma_(i=1)^(n) X_i";
        let mut rows = search(typed, &[], &[], false);
        assert_eq!(rows[0].kind, Kind::Equation);
        assert!(rows[0].text.starts_with("x² + 2x/8"));
        add_app_math(&mut rows, typed, true);
        // Unicode cannot write x^(3/2): the app's equation comes first.
        assert_eq!(rows[0].kind, Kind::EquationIn);
        assert!(rows[0]
            .text
            .starts_with("{press Alt+=}{wait 400}x^2 + (2x)/8"));
        let steps = crate::macros::steps(&rows[0].text).unwrap().unwrap();
        assert_eq!(steps[0], crate::macros::Step::Keys("Alt+=".into()));
        let f = crate::macros::steps(&rows[1].text).unwrap().unwrap();
        assert!(f.contains(&crate::macros::Step::Text("3/2".into())));
        assert_eq!(rows[2].kind, Kind::Equation);
        // All in Unicode: that row stays first.
        let mut rows = search("x^2+1", &[], &[], false);
        add_app_math(&mut rows, "x^2+1", false);
        assert_eq!(rows[0].kind, Kind::Equation);
        assert_eq!(rows[1].kind, Kind::EquationIn);
    }

    #[test]
    fn math_said_in_words() {
        let rows = search("x squared plus 1", &[], &[], false);
        assert_eq!(rows[0].kind, Kind::Equation);
        assert_eq!(rows[0].text, "x² + 1");
        let rows = search("รากที่สองของ x", &[], &[], true);
        assert_eq!(rows[0].text, "√x");
        assert_eq!(rows[0].detail, "คณิต");
    }

    #[test]
    fn characters_and_snippets_together() {
        let rows = search("degree", &sig(), &[], false);
        assert_eq!(rows[0].glyph, "°");
        assert_eq!(rows[0].detail, "U+00B0");
        assert_eq!(rows[0].kind, Kind::Character);
        let rows = search("regards", &sig(), &[], false);
        assert_eq!(rows[0].kind, Kind::Snippet);
        assert_eq!(rows[0].label, "Best regards, Affan");
        assert_eq!(rows[0].text, "Best regards,\nAffan");
    }

    #[test]
    fn latex_first() {
        let rows = search(r"\frac{1}{2} + x^2", &[], &[], false);
        assert_eq!(rows[0].kind, Kind::Equation);
        assert_eq!(rows[0].text, "½ + x²");
        let rows = search(r"\frac{\frac{1}{2}}{3}", &[], &[], true);
        assert_eq!(rows[0].kind, Kind::Equation);
        assert!(rows[0].text.is_empty(), "nothing to pick");
        let rows = search(r"\alp", &[], &[], false);
        assert_eq!(rows[0].glyph, "α");
        assert_eq!(rows[0].label, r"\alpha");
        assert!(search("arrow", &[], &[], false)
            .iter()
            .all(|r| r.kind != Kind::Equation));
    }

    #[test]
    fn app_commands() {
        let cmds = vec![
            Command {
                name: "Replace".into(),
                keys: "Ctrl+H".into(),
                other: "แทนที่".into(),
            },
            Command {
                name: "Close the tab".into(),
                keys: "Ctrl+W".into(),
                other: "ปิดแท็บ".into(),
            },
        ];
        let rows = search("replace", &[], &cmds, false);
        assert_eq!(rows[0].kind, Kind::Command);
        assert_eq!(rows[0].detail, "Ctrl+H");
        let rows = search("close", &[], &cmds, false);
        assert_eq!(rows[0].label, "Close the tab", "before close parenthesis");
        assert!(!needs_confirming(&cmds[0]));
        assert!(needs_confirming(&cmds[1]));
        assert!(!needs_confirming(&Command {
            name: "Bookmark the page".into(),
            keys: "Ctrl+D".into(),
            other: String::new(),
        }));
        assert!(needs_confirming(&Command {
            name: "ลบไฟล์".into(),
            keys: "Del".into(),
            other: String::new(),
        }));
        let rows = search("แทนที่", &[], &cmds, false);
        assert_eq!(rows[0].label, "Replace", "found by its other name");
    }

    #[test]
    fn thai_names_in_the_thai_interface() {
        let rows = search("ยูโร", &[], &[], true);
        let euro = rows.iter().find(|r| r.glyph == "€").expect("€");
        assert!(euro.label.contains("ยูโร"), "{}", euro.label);
        assert!(search("", &sig(), &[], true).is_empty());
        assert!(search("anything at all", &[], &[], false).len() <= MAX_ROWS);
    }
}
