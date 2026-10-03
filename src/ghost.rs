//! Ghost suggestions (2.4 C1): a few characters people type to stand for a
//! symbol (`->` for →, `!=` for ≠, `x^2` for x²) are offered, faintly, next
//! to the cursor; Tab takes the offer, anything else leaves the text as
//! typed. Never changed without Tab.
//!
//! No OS calls; the text looked at is the end of what the keyboard hook
//! already holds.

/// What the typist typed, and what Tab puts instead. Not `(c)`, `(r)`,
/// `(tm)`: `(a) (b) (c)` is a list far more often than ©, and the list at
/// the cursor finds ©, ®, ™ by name.
pub const RULES: &[(&str, &str)] = &[
    ("<->", "↔"),
    ("<=>", "⇔"),
    ("->", "→"),
    ("<-", "←"),
    ("=>", "⇒"),
    ("<=", "≤"),
    (">=", "≥"),
    ("!=", "≠"),
    ("+-", "±"),
    ("~=", "≈"),
    ("1/2", "½"),
    ("1/3", "⅓"),
    ("2/3", "⅔"),
    ("1/4", "¼"),
    ("3/4", "¾"),
    ("...", "…"),
    ("--", "–"),
    ("---", "—"),
];

/// An offer: replace the last `replace` characters with `text`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ghost {
    pub replace: usize,
    pub text: String,
}

fn superscript(c: char) -> Option<char> {
    Some(match c {
        '0' => '⁰',
        '1' => '¹',
        '2' => '²',
        '3' => '³',
        '4' => '⁴',
        '5' => '⁵',
        '6' => '⁶',
        '7' => '⁷',
        '8' => '⁸',
        '9' => '⁹',
        '+' => '⁺',
        '-' => '⁻',
        'n' => 'ⁿ',
        'i' => 'ⁱ',
        _ => return None,
    })
}

/// The offer for text ending in `before` (what is before the cursor), if
/// any. A rule only counts when what comes before it is not more of the
/// same kind: `<--` is not `<-` then `-`, and `11/2` is not `1` then ½.
pub fn offer(before: &str) -> Option<Ghost> {
    // x^2, a^n: a letter or digit, ^, then digits (or n, i).
    if let Some(at) = before.rfind('^') {
        let (head, tail) = (&before[..at], &before[at + 1..]);
        let base = head.chars().next_back();
        if base.is_some_and(|c| c.is_alphanumeric() || c == ')')
            && !tail.is_empty()
            && tail.chars().count() <= 3
        {
            let sup: Option<String> = tail.chars().map(superscript).collect();
            if let Some(sup) = sup {
                return Some(Ghost {
                    replace: tail.chars().count() + 1,
                    text: sup,
                });
            }
        }
    }
    // \alpha, \leq, \infty: a LaTeX command for one symbol.
    if let Some(at) = before.rfind('\\') {
        let name = &before[at + 1..];
        if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphabetic()) {
            if let Some(symbol) =
                crate::latex::symbol(name).filter(|s| *s != name && !s.trim().is_empty())
            {
                return Some(Ghost {
                    replace: name.len() + 1,
                    text: symbol.to_string(),
                });
            }
        }
    }
    // The longest rule the text ends with.
    let (typed, text) = RULES
        .iter()
        .filter(|(t, _)| before.ends_with(t))
        .max_by_key(|(t, _)| t.len())?;
    let rest = &before[..before.len() - typed.len()];
    let prev = rest.chars().next_back();
    let first = typed.chars().next()?;
    let blocked = match prev {
        None => false,
        // Part of a longer run of symbols: `<--`, `!==`, `....`, `=->`
        // (an opening bracket or quote before it is fine: `(->)`).
        Some(p)
            if !first.is_alphanumeric()
                && p.is_ascii_punctuation()
                && !matches!(p, '(' | '[' | '{' | '"' | '\'') =>
        {
            true
        }
        // A fraction inside a bigger number or date: `11/2`, `2/1/2`.
        Some(p) if first.is_ascii_digit() => p.is_ascii_digit() || p == '/' || p == '.',
        _ => false,
    };
    (!blocked).then(|| Ghost {
        replace: typed.chars().count(),
        text: (*text).to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn o(s: &str) -> Option<(usize, String)> {
        offer(s).map(|g| (g.replace, g.text))
    }

    #[test]
    fn offers_the_symbol() {
        assert_eq!(o("a -> b ->"), Some((2, "→".into())));
        assert_eq!(o("x != y !="), Some((2, "≠".into())));
        assert_eq!(o("<->"), Some((3, "↔".into())));
        assert_eq!(o("about 1/2"), Some((3, "½".into())));
        assert_eq!(o("wait..."), Some((3, "…".into())));
        assert_eq!(o("E = mc^2"), Some((2, "²".into())));
        assert_eq!(o("x^10"), Some((3, "¹⁰".into())));
        assert_eq!(o("a^n"), Some((2, "ⁿ".into())));
        assert_eq!(o(r"let \alpha"), Some((6, "α".into())));
        assert_eq!(o(r"\Rightarrow"), Some((11, "⇒".into())));
    }

    #[test]
    fn leaves_what_is_something_else() {
        assert_eq!(o("hello"), None);
        assert_eq!(o("<--"), None, "an arrow of its own");
        assert_eq!(o("=->"), None);
        assert_eq!(o("(->"), Some((2, "→".into())));
        assert_eq!(o("11/2"), None, "a bigger number");
        assert_eq!(o("1/1/2"), None, "a date");
        assert_eq!(o("v1.1/2"), None);
        assert_eq!(o("(a) (b) (c)"), None, "a list, not ©");
        assert_eq!(o("x^"), None);
        assert_eq!(o("^2"), None, "nothing to raise");
        assert_eq!(o("x^abc"), None);
        assert_eq!(o(r"\sin"), None, "a function name stays a word");
        assert_eq!(o(r"\alphabet"), None);
        assert_eq!(o(r"\quad"), None, "a space is not a symbol to offer");
        assert_eq!(o("...."), None, "more dots than an ellipsis");
    }
}
