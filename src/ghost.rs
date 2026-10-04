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

/// How many characters before the caret are kept: a bit of math like
/// `x^2+y^2=z^2`, the longest LaTeX command (`\\Leftrightarrow`), or math
/// said in words (`x ยกกำลังสองบวก 1`).
pub const TAIL: usize = 48;

/// What the key being handled did to the [`Tail`], so that text RightType
/// writes meanwhile lands before it, and it can be taken back if the key
/// never reaches the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pending {
    None,
    Pushed(char),
    Popped(Option<char>),
}

/// How a write ends (the key RightType types after its text).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Then {
    Nothing,
    Space,
    /// Enter, Tab…: the text before the caret is another line now.
    Other,
}

/// The text before the caret, as the keyboard hook follows it: what the
/// typist types and what RightType writes over it (a word put into the
/// other layout, a snippet, a ghost taken). In memory only; zeroized when
/// let go.
#[derive(Debug)]
pub struct Tail {
    text: String,
    pending: Pending,
}

impl Default for Tail {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Tail {
    fn drop(&mut self) {
        self.forget();
    }
}

impl Tail {
    pub const fn new() -> Self {
        Self {
            text: String::new(),
            pending: Pending::None,
        }
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }

    pub fn last(&self) -> Option<char> {
        self.text.chars().next_back()
    }

    fn trim(&mut self) {
        let n = self.text.chars().count();
        if n > TAIL {
            let cut = self
                .text
                .char_indices()
                .nth(n - TAIL)
                .map_or(self.text.len(), |(i, _)| i);
            // Zeroized as it goes: the bytes are not left behind.
            let mut gone: String = self.text.drain(..cut).collect();
            zeroize_string(&mut gone);
        }
    }

    /// Forget everything (Enter, an arrow, a click, another field).
    pub fn forget(&mut self) {
        zeroize_string(&mut self.text);
        self.pending = Pending::None;
    }

    /// A key starts: none of it is pending yet.
    pub fn begin_key(&mut self) {
        self.pending = Pending::None;
    }

    /// A character typed (pending until the key is settled).
    pub fn typed(&mut self, c: char) {
        self.text.push(c);
        self.trim();
        self.pending = Pending::Pushed(c);
    }

    /// Backspace typed (pending until the key is settled).
    pub fn backspace(&mut self) {
        self.pending = Pending::Popped(self.text.pop());
    }

    fn undo_pending(&mut self) {
        match self.pending {
            Pending::Pushed(c) if self.text.ends_with(c) => {
                self.text.pop();
            }
            Pending::Popped(Some(c)) => self.text.push(c),
            _ => {}
        }
    }

    fn redo_pending(&mut self) {
        match self.pending {
            Pending::Pushed(c) => self.text.push(c),
            // What it takes off now is what to give back if it is kept.
            Pending::Popped(_) => self.pending = Pending::Popped(self.text.pop()),
            Pending::None => {}
        }
    }

    /// RightType wrote `text` over the `backspaces` characters before the
    /// caret, then `then`. The key being handled, if it reaches the app,
    /// comes after.
    pub fn written(&mut self, backspaces: usize, text: &str, then: Then) {
        self.undo_pending();
        for _ in 0..backspaces {
            self.text.pop();
        }
        self.text.push_str(text);
        match then {
            Then::Nothing => {}
            Then::Space => self.text.push(' '),
            Then::Other => {
                self.forget();
                return;
            }
        }
        self.redo_pending();
        self.trim();
    }

    /// The key has been handled: if it was kept from the app, take back
    /// what it did.
    pub fn settle(&mut self, kept_from_app: bool) {
        if kept_from_app {
            self.undo_pending();
        }
        self.pending = Pending::None;
    }
}

fn zeroize_string(s: &mut String) {
    zeroize::Zeroize::zeroize(s);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(t: &mut Tail, s: &str) {
        for c in s.chars() {
            t.begin_key();
            if c == ' ' {
                t.typed(' ');
            } else {
                t.typed(c);
            }
            t.settle(false);
        }
    }

    #[test]
    fn the_tail_follows_what_is_written() {
        // `x de]y'` typed; the word put into Thai at the space (the space
        // typed by RightType after it, the key kept from the app).
        let mut t = Tail::new();
        typed(&mut t, "x de]y'");
        t.begin_key();
        t.typed(' ');
        t.written(5, "กำลัง", Then::Space);
        t.settle(true);
        assert_eq!(t.as_str(), "x กำลัง ");
        typed(&mut t, "10");
        assert_eq!(t.as_str(), "x กำลัง 10");
        assert_eq!(offer(t.as_str()).map(|g| g.text).as_deref(), Some("x¹⁰"));

        // Put into Thai as it is typed: each key kept, the run redrawn.
        let mut t = Tail::new();
        typed(&mut t, "x ");
        t.begin_key();
        t.typed('d');
        t.settle(false);
        t.begin_key();
        t.typed('e');
        t.written(1, "กำ", Then::Nothing);
        t.settle(true);
        assert_eq!(t.as_str(), "x กำ");

        // A key that goes on to the app after a write lands after it.
        let mut t = Tail::new();
        typed(&mut t, "ab");
        t.begin_key();
        t.typed('c');
        t.written(2, "XY", Then::Nothing);
        t.settle(false);
        assert_eq!(t.as_str(), "XYc");

        // Backspace kept from the app (an undo) is taken back.
        let mut t = Tail::new();
        typed(&mut t, "ab");
        t.begin_key();
        t.backspace();
        t.written(1, "Z", Then::Nothing);
        t.settle(true);
        assert_eq!(t.as_str(), "aZ");

        // Enter after a write: another line.
        t.begin_key();
        t.written(0, "x", Then::Other);
        t.settle(false);
        assert_eq!(t.as_str(), "");

        // Kept to TAIL characters.
        let mut t = Tail::new();
        typed(&mut t, &"ก".repeat(TAIL + 10));
        assert_eq!(t.as_str().chars().count(), TAIL);
    }

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
