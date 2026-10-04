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
    // A whole bit of math since the last space with more than one script in
    // it — (x^2)^2, x^2+y^2, ^1234 — written as Unicode at once.
    if let Some(g) = math(before) {
        return Some(g);
    }
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
    // The longest rule the text ends with; else math said in words
    // (`x ยกกำลังสองบวก 1`).
    if let Some(g) = rule(before) {
        return Some(g);
    }
    crate::naturalmath::offer(before).map(|(replace, text)| Ghost { replace, text })
}

fn rule(before: &str) -> Option<Ghost> {
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

/// The math at the end of `before` (since its last space), when it has two
/// or more superscripts or subscripts: (x^2)^2 is (x²)², ^1234 is ¹²³⁴.
/// Only `^` before a digit, sign, bracket, n or i, and `_` before a digit
/// or `{`: `snake_case` and `my_var` are not math.
fn math(before: &str) -> Option<Ghost> {
    let start = before.rfind(char::is_whitespace).map_or(0, |i| i + 1);
    let token = &before[start..];
    let chars: Vec<char> = token.chars().collect();
    let mut scripts = 0;
    for (i, &c) in chars.iter().enumerate() {
        let next = chars.get(i + 1).copied();
        match c {
            '^' => {
                if !next.is_some_and(|n| n.is_ascii_digit() || "{(+-ni".contains(n)) {
                    return None;
                }
                scripts += 1;
            }
            '_' => {
                if !next.is_some_and(|n| n.is_ascii_digit() || n == '{') {
                    return None;
                }
                scripts += 1;
            }
            '\\' => return None,
            _ => {}
        }
    }
    // One script alone is the rule below (`x^2`), and a lone `^12` with
    // nothing to raise is still math when it has two digits or more.
    let lone = token.starts_with('^') && chars.len() >= 3;
    if scripts < 2 && !lone {
        return None;
    }
    // As typed, not as LaTeX reads it: x^10 raises 10, not 1.
    let mut braced = String::new();
    let mut it = token.chars().peekable();
    while let Some(c) = it.next() {
        braced.push(c);
        if (c == '^' || c == '_') && it.peek().is_some_and(|n| n.is_ascii_digit()) {
            braced.push('{');
            while let Some(&d) = it.peek().filter(|d| d.is_ascii_digit()) {
                braced.push(d);
                it.next();
            }
            braced.push('}');
        }
    }
    let text = crate::latex::to_unicode(&braced).ok()?;
    (text != token && !text.contains(['^', '_'])).then_some(Ghost {
        replace: chars.len(),
        text,
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
        // Math as a whole.
        assert_eq!(o("so (x^2)^2"), Some((7, "(x²)²".into())));
        assert_eq!(o("x^2+y^2"), Some((7, "x²+y²".into())));
        assert_eq!(o("^1234"), Some((5, "¹²³⁴".into())));
        assert_eq!(o("H_2O_2"), Some((6, "H₂O₂".into())));
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
        assert_eq!(o("my_var_2"), None, "an identifier is not math");
        assert_eq!(o("x^10+1^2"), Some((8, "x¹⁰+1²".into())));
        assert_eq!(o(r"\sin"), None, "a function name stays a word");
        assert_eq!(o(r"\alphabet"), None);
        assert_eq!(o(r"\quad"), None, "a space is not a symbol to offer");
        assert_eq!(o("...."), None, "more dots than an ellipsis");
    }
}
