//! LaTeX math as plain Unicode text (2.4 D1): `\alpha^2 \leq \frac{1}{2}`
//! becomes `α² ≤ ½`, for apps with no equation editor (chat, mail, notes).
//!
//! Only what one line of Unicode can say honestly: Greek, operators, arrows,
//! sets, superscripts and subscripts Unicode has, `\sqrt`, simple `\frac`,
//! accents. Anything else (a nested fraction, a matrix, `x^q` with no
//! superscript q) is an [`Error`] saying what, never a guess.
//!
//! No OS calls.

use std::fmt;

/// Why the source cannot be one line of Unicode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// A command not known here: `\foo`.
    Unknown(String),
    /// Unicode has no superscript (or subscript) of this character.
    NoSuperscript(char),
    NoSubscript(char),
    /// A fraction inside a fraction, a root of a root index.
    Nested,
    /// `\begin{...}`: matrices, aligned equations and the like.
    Environment,
    /// A `{` without its `}`, or `^` with nothing after it.
    Unbalanced,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown(name) => write!(f, "\\{name} is not known"),
            Self::NoSuperscript(c) => write!(f, "no superscript {c} in Unicode"),
            Self::NoSubscript(c) => write!(f, "no subscript {c} in Unicode"),
            Self::Nested => write!(f, "a fraction inside a fraction"),
            Self::Environment => write!(f, "\\begin…\\end needs an equation editor"),
            Self::Unbalanced => write!(f, "a {{ without its }}, or ^ _ with nothing after"),
        }
    }
}

impl Error {
    /// What the list at the cursor says, in the interface language.
    pub fn describe(&self, thai: bool) -> String {
        if !thai {
            return format!("Cannot write as one line: {self}");
        }
        match self {
            Self::Unknown(name) => format!("ไม่รู้จัก \\{name}"),
            Self::NoSuperscript(c) => format!("Unicode ไม่มีตัวยก {c}"),
            Self::NoSubscript(c) => format!("Unicode ไม่มีตัวห้อย {c}"),
            Self::Nested => "เศษส่วนซ้อนกันเขียนเป็นบรรทัดเดียวไม่ได้".into(),
            Self::Environment => "\\begin…\\end ต้องใช้ตัวเขียนสมการ".into(),
            Self::Unbalanced => "ยังพิมพ์ไม่ครบ: { ไม่มี } หรือ ^ _ ไม่มีตัวตาม".into(),
        }
    }
}

/// Commands that stand for one symbol.
const SYMBOLS: &[(&str, &str)] = &[
    // Greek, small.
    ("alpha", "α"),
    ("beta", "β"),
    ("gamma", "γ"),
    ("delta", "δ"),
    ("epsilon", "ϵ"),
    ("varepsilon", "ε"),
    ("zeta", "ζ"),
    ("eta", "η"),
    ("theta", "θ"),
    ("vartheta", "ϑ"),
    ("iota", "ι"),
    ("kappa", "κ"),
    ("lambda", "λ"),
    ("mu", "μ"),
    ("nu", "ν"),
    ("xi", "ξ"),
    ("omicron", "ο"),
    ("pi", "π"),
    ("varpi", "ϖ"),
    ("rho", "ρ"),
    ("varrho", "ϱ"),
    ("sigma", "σ"),
    ("varsigma", "ς"),
    ("tau", "τ"),
    ("upsilon", "υ"),
    ("phi", "ϕ"),
    ("varphi", "φ"),
    ("chi", "χ"),
    ("psi", "ψ"),
    ("omega", "ω"),
    // Greek, capital (those that differ from Latin).
    ("Gamma", "Γ"),
    ("Delta", "Δ"),
    ("Theta", "Θ"),
    ("Lambda", "Λ"),
    ("Xi", "Ξ"),
    ("Pi", "Π"),
    ("Sigma", "Σ"),
    ("Upsilon", "Υ"),
    ("Phi", "Φ"),
    ("Psi", "Ψ"),
    ("Omega", "Ω"),
    // Operators and relations.
    ("times", "×"),
    ("div", "÷"),
    ("pm", "±"),
    ("mp", "∓"),
    ("cdot", "⋅"),
    ("ast", "∗"),
    ("star", "⋆"),
    ("circ", "∘"),
    ("bullet", "•"),
    ("oplus", "⊕"),
    ("ominus", "⊖"),
    ("otimes", "⊗"),
    ("leq", "≤"),
    ("le", "≤"),
    ("geq", "≥"),
    ("ge", "≥"),
    ("neq", "≠"),
    ("ne", "≠"),
    ("ll", "≪"),
    ("gg", "≫"),
    ("approx", "≈"),
    ("equiv", "≡"),
    ("sim", "∼"),
    ("simeq", "≃"),
    ("cong", "≅"),
    ("propto", "∝"),
    ("prec", "≺"),
    ("succ", "≻"),
    ("mid", "∣"),
    ("parallel", "∥"),
    ("perp", "⊥"),
    // Big operators and calculus.
    ("sum", "∑"),
    ("prod", "∏"),
    ("coprod", "∐"),
    ("int", "∫"),
    ("iint", "∬"),
    ("iiint", "∭"),
    ("oint", "∮"),
    ("partial", "∂"),
    ("nabla", "∇"),
    ("infty", "∞"),
    // Logic and sets.
    ("forall", "∀"),
    ("exists", "∃"),
    ("nexists", "∄"),
    ("in", "∈"),
    ("notin", "∉"),
    ("ni", "∋"),
    ("subset", "⊂"),
    ("supset", "⊃"),
    ("subseteq", "⊆"),
    ("supseteq", "⊇"),
    ("cup", "∪"),
    ("cap", "∩"),
    ("setminus", "∖"),
    ("emptyset", "∅"),
    ("varnothing", "∅"),
    ("land", "∧"),
    ("wedge", "∧"),
    ("lor", "∨"),
    ("vee", "∨"),
    ("neg", "¬"),
    ("lnot", "¬"),
    ("therefore", "∴"),
    ("because", "∵"),
    ("top", "⊤"),
    ("bot", "⊥"),
    ("vdash", "⊢"),
    ("models", "⊨"),
    // Arrows.
    ("to", "→"),
    ("rightarrow", "→"),
    ("leftarrow", "←"),
    ("gets", "←"),
    ("leftrightarrow", "↔"),
    ("uparrow", "↑"),
    ("downarrow", "↓"),
    ("Rightarrow", "⇒"),
    ("Leftarrow", "⇐"),
    ("Leftrightarrow", "⇔"),
    ("implies", "⟹"),
    ("iff", "⟺"),
    ("mapsto", "↦"),
    ("longrightarrow", "⟶"),
    ("longleftarrow", "⟵"),
    // Brackets.
    ("langle", "⟨"),
    ("rangle", "⟩"),
    ("lceil", "⌈"),
    ("rceil", "⌉"),
    ("lfloor", "⌊"),
    ("rfloor", "⌋"),
    ("lbrace", "{"),
    ("rbrace", "}"),
    // Dots.
    ("ldots", "…"),
    ("dots", "…"),
    ("cdots", "⋯"),
    ("vdots", "⋮"),
    ("ddots", "⋱"),
    // Letters and others.
    ("aleph", "ℵ"),
    ("hbar", "ℏ"),
    ("ell", "ℓ"),
    ("Re", "ℜ"),
    ("Im", "ℑ"),
    ("wp", "℘"),
    ("angle", "∠"),
    ("triangle", "△"),
    ("square", "□"),
    ("degree", "°"),
    ("prime", "′"),
    ("checkmark", "✓"),
    ("dagger", "†"),
    ("S", "§"),
    ("P", "¶"),
    ("copyright", "©"),
    ("pounds", "£"),
    ("euro", "€"),
    // Spacing.
    ("quad", "\u{2003}"),
    ("qquad", "\u{2003}\u{2003}"),
    // Function names are written upright, as words.
    ("sin", "sin"),
    ("cos", "cos"),
    ("tan", "tan"),
    ("cot", "cot"),
    ("sec", "sec"),
    ("csc", "csc"),
    ("arcsin", "arcsin"),
    ("arccos", "arccos"),
    ("arctan", "arctan"),
    ("sinh", "sinh"),
    ("cosh", "cosh"),
    ("tanh", "tanh"),
    ("log", "log"),
    ("ln", "ln"),
    ("lg", "lg"),
    ("exp", "exp"),
    ("lim", "lim"),
    ("max", "max"),
    ("min", "min"),
    ("sup", "sup"),
    ("inf", "inf"),
    ("det", "det"),
    ("gcd", "gcd"),
    ("deg", "deg"),
    ("dim", "dim"),
    ("ker", "ker"),
    ("arg", "arg"),
    ("Pr", "Pr"),
];

/// The symbol a command stands for, if it stands for one.
pub fn symbol(name: &str) -> Option<&'static str> {
    SYMBOLS.iter().find(|(n, _)| *n == name).map(|(_, s)| *s)
}

/// Every command name known here, for search and completion.
pub fn names() -> impl Iterator<Item = (&'static str, &'static str)> {
    SYMBOLS.iter().copied()
}

fn superscript(c: char) -> Option<char> {
    const FROM: &str = "0123456789+-=()abcdefghijklmnoprstuvwxyzABDEGHIJKLMNOPRTUVWαβγδεθιφχ";
    const TO: &str = "⁰¹²³⁴⁵⁶⁷⁸⁹⁺⁻⁼⁽⁾ᵃᵇᶜᵈᵉᶠᵍʰⁱʲᵏˡᵐⁿᵒᵖʳˢᵗᵘᵛʷˣʸᶻᴬᴮᴰᴱᴳᴴᴵᴶᴷᴸᴹᴺᴼᴾᴿᵀᵁⱽᵂᵅᵝᵞᵟᵋᶿᶥᵠᵡ";
    let c = if c == '−' { '-' } else { c };
    FROM.chars()
        .position(|f| f == c)
        .and_then(|i| TO.chars().nth(i))
}

fn subscript(c: char) -> Option<char> {
    const FROM: &str = "0123456789+-=()aehijklmnoprstuvxβγρφχ";
    const TO: &str = "₀₁₂₃₄₅₆₇₈₉₊₋₌₍₎ₐₑₕᵢⱼₖₗₘₙₒₚᵣₛₜᵤᵥₓᵦᵧᵨᵩᵪ";
    let c = if c == '−' { '-' } else { c };
    FROM.chars()
        .position(|f| f == c)
        .and_then(|i| TO.chars().nth(i))
}

fn double_struck(c: char) -> Option<char> {
    Some(match c {
        'C' => 'ℂ',
        'H' => 'ℍ',
        'N' => 'ℕ',
        'P' => 'ℙ',
        'Q' => 'ℚ',
        'R' => 'ℝ',
        'Z' => 'ℤ',
        'A'..='Z' => char::from_u32(0x1D538 + (c as u32 - 'A' as u32))?,
        'a'..='z' => char::from_u32(0x1D552 + (c as u32 - 'a' as u32))?,
        '0'..='9' => char::from_u32(0x1D7D8 + (c as u32 - '0' as u32))?,
        _ => return None,
    })
}

fn vulgar(num: &str, den: &str) -> Option<&'static str> {
    Some(match (num, den) {
        ("1", "2") => "½",
        ("1", "3") => "⅓",
        ("2", "3") => "⅔",
        ("1", "4") => "¼",
        ("3", "4") => "¾",
        ("1", "5") => "⅕",
        ("2", "5") => "⅖",
        ("3", "5") => "⅗",
        ("4", "5") => "⅘",
        ("1", "6") => "⅙",
        ("5", "6") => "⅚",
        ("1", "7") => "⅐",
        ("1", "8") => "⅛",
        ("3", "8") => "⅜",
        ("5", "8") => "⅝",
        ("7", "8") => "⅞",
        ("1", "9") => "⅑",
        ("1", "10") => "⅒",
        _ => return None,
    })
}

struct Parser<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
    /// Inside a `\frac`: another one is [`Error::Nested`].
    in_frac: bool,
}

impl Parser<'_> {
    /// Up to the end, or the `}` closing the group being read.
    fn sequence(&mut self, in_group: bool) -> Result<String, Error> {
        let mut out = String::new();
        loop {
            match self.chars.peek().copied() {
                None if in_group => return Err(Error::Unbalanced),
                None => return Ok(out),
                Some('}') if in_group => {
                    self.chars.next();
                    return Ok(out);
                }
                Some('}') => return Err(Error::Unbalanced),
                Some('^') | Some('_') => {
                    let up = self.chars.next() == Some('^');
                    let arg = self.argument()?;
                    for c in arg.chars() {
                        let mapped = if up { superscript(c) } else { subscript(c) };
                        match mapped {
                            Some(m) => out.push(m),
                            None if c == ' ' => {}
                            None if up => return Err(Error::NoSuperscript(c)),
                            None => return Err(Error::NoSubscript(c)),
                        }
                    }
                }
                Some(_) => out.push_str(&self.atom()?),
            }
        }
    }

    /// One thing: a command, a `{group}`, or a character.
    fn atom(&mut self) -> Result<String, Error> {
        match self.chars.next() {
            None => Err(Error::Unbalanced),
            Some('{') => self.sequence(true),
            Some('\\') => self.command(),
            Some('-') => Ok("−".into()),
            Some('\'') => Ok("′".into()),
            Some(c) => Ok(c.to_string()),
        }
    }

    /// What `^`, `_` or a command's argument takes: a group or one atom
    /// (spaces before it skipped).
    fn argument(&mut self) -> Result<String, Error> {
        while self.chars.peek() == Some(&' ') {
            self.chars.next();
        }
        match self.chars.peek() {
            None | Some('}') | Some('^') | Some('_') => Err(Error::Unbalanced),
            _ => self.atom(),
        }
    }

    /// `[3]` after `\sqrt`, if there is one.
    fn optional(&mut self) -> Result<Option<String>, Error> {
        if self.chars.peek() != Some(&'[') {
            return Ok(None);
        }
        self.chars.next();
        let mut inside = String::new();
        loop {
            match self.chars.next() {
                None => return Err(Error::Unbalanced),
                Some(']') => return Ok(Some(inside)),
                Some(c) => inside.push(c),
            }
        }
    }

    fn command(&mut self) -> Result<String, Error> {
        let mut name = String::new();
        while let Some(&c) = self.chars.peek() {
            if c.is_ascii_alphabetic() {
                name.push(c);
                self.chars.next();
            } else {
                break;
            }
        }
        if name.is_empty() {
            // `\{`, `\,`, `\\`: one character after the backslash.
            return match self.chars.next() {
                Some(c @ ('{' | '}' | '%' | '$' | '&' | '#' | '_')) => Ok(c.to_string()),
                Some(',' | ':' | ';' | '>') => Ok("\u{2009}".into()),
                Some(' ') => Ok(" ".into()),
                Some('!') => Ok(String::new()),
                Some('|') => Ok("‖".into()),
                Some(c) => Err(Error::Unknown(c.to_string())),
                None => Err(Error::Unbalanced),
            };
        }
        if let Some(s) = symbol(&name) {
            return Ok(s.to_string());
        }
        match name.as_str() {
            "frac" | "dfrac" | "tfrac" => {
                if self.in_frac {
                    return Err(Error::Nested);
                }
                self.in_frac = true;
                let num = self.argument();
                let den = self.argument();
                self.in_frac = false;
                fraction(&num?, &den?)
            }
            "sqrt" => {
                let index = self.optional()?;
                let sign = match index.as_deref().map(str::trim) {
                    None | Some("2") => "√",
                    Some("3") => "∛",
                    Some("4") => "∜",
                    Some(_) => return Err(Error::Nested),
                };
                let arg = self.argument()?;
                Ok(if arg.chars().count() == 1 {
                    format!("{sign}{arg}")
                } else {
                    format!("{sign}({arg})")
                })
            }
            "mathbb" => self
                .argument()?
                .chars()
                .map(|c| double_struck(c).ok_or_else(|| Error::Unknown(format!("mathbb{{{c}}}"))))
                .collect(),
            "text" | "mathrm" | "mathit" | "mathbf" | "mathsf" | "mathtt" | "operatorname"
            | "textrm" | "textbf" | "textit" | "boldsymbol" | "displaystyle" => {
                if name == "displaystyle" {
                    Ok(String::new())
                } else {
                    self.argument()
                }
            }
            "left" | "right" | "big" | "Big" | "bigg" | "Bigg" | "bigl" | "bigr" | "Bigl"
            | "Bigr" => {
                // Only the bracket after it: `\left(` is `(`, `\left.` nothing.
                match self.chars.peek() {
                    Some('.') => {
                        self.chars.next();
                        Ok(String::new())
                    }
                    _ => self.argument(),
                }
            }
            "hat" | "widehat" | "bar" | "overline" | "vec" | "dot" | "ddot" | "tilde"
            | "widetilde" | "underline" => {
                let mark = match name.as_str() {
                    "hat" | "widehat" => '\u{0302}',
                    "bar" | "overline" => '\u{0305}',
                    "vec" => '\u{20D7}',
                    "dot" => '\u{0307}',
                    "ddot" => '\u{0308}',
                    "tilde" | "widetilde" => '\u{0303}',
                    _ => '\u{0332}',
                };
                let arg = self.argument()?;
                let mut out = String::new();
                for c in arg.chars() {
                    out.push(c);
                    out.push(mark);
                }
                Ok(out)
            }
            "begin" | "end" | "matrix" | "pmatrix" | "bmatrix" | "cases" | "array" => {
                Err(Error::Environment)
            }
            _ => Err(Error::Unknown(name)),
        }
    }
}

fn fraction(num: &str, den: &str) -> Result<String, Error> {
    if let Some(v) = vulgar(num.trim(), den.trim()) {
        return Ok(v.to_string());
    }
    let simple = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_alphanumeric());
    if num.chars().all(|c| c.is_ascii_digit()) && den.chars().all(|c| c.is_ascii_digit()) {
        // ¹²⁄₃₅, the way a typesetter writes a fraction on one line.
        if let (Some(n), Some(d)) = (
            num.chars().map(superscript).collect::<Option<String>>(),
            den.chars().map(subscript).collect::<Option<String>>(),
        ) {
            if !n.is_empty() && !d.is_empty() {
                return Ok(format!("{n}⁄{d}"));
            }
        }
    }
    let wrap = |s: &str| {
        if simple(s) {
            s.to_string()
        } else {
            format!("({s})")
        }
    };
    if num.is_empty() || den.is_empty() {
        return Err(Error::Unbalanced);
    }
    Ok(format!("{}/{}", wrap(num), wrap(den)))
}

/// `src` (LaTeX math, with or without `$…$`) as one line of Unicode.
pub fn to_unicode(src: &str) -> Result<String, Error> {
    let src = src.trim();
    let src = src
        .strip_prefix("$$")
        .and_then(|s| s.strip_suffix("$$"))
        .or_else(|| src.strip_prefix('$').and_then(|s| s.strip_suffix('$')))
        .or_else(|| src.strip_prefix("\\(").and_then(|s| s.strip_suffix("\\)")))
        .unwrap_or(src);
    let mut p = Parser {
        chars: src.chars().peekable(),
        in_frac: false,
    };
    p.sequence(false)
}

/// Whether `s` is meant as LaTeX: it has a command (`\alpha`) or a
/// superscript/subscript after something (`x^2`, `a_n`).
pub fn looks_like_latex(s: &str) -> bool {
    let mut prev: Option<char> = None;
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '\\' if it.peek().is_some_and(|n| n.is_ascii_alphabetic()) => return true,
            '^' | '_'
                if prev.is_some_and(|p| p.is_alphanumeric() || matches!(p, ')' | '}'))
                    && it.peek().is_some_and(|n| !n.is_whitespace()) =>
            {
                return true
            }
            _ => {}
        }
        prev = Some(c);
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(s: &str) -> String {
        to_unicode(s).unwrap_or_else(|e| panic!("{s}: {e}"))
    }

    #[test]
    fn symbols_and_scripts() {
        assert_eq!(u(r"\alpha^2 \leq \frac{1}{2}"), "α² ≤ ½");
        assert_eq!(u(r"E = mc^2"), "E = mc²");
        assert_eq!(u(r"a_n + a_{n+1}"), "aₙ + aₙ₊₁");
        assert_eq!(u(r"\sum_{i=1}^{n} x_i"), "∑ᵢ₌₁ⁿ xᵢ");
        assert_eq!(u(r"x^{-1}"), "x⁻¹");
        assert_eq!(u(r"$\forall x \in \mathbb{R}$"), "∀ x ∈ ℝ");
        assert_eq!(u(r"\int_0^1 e^{-x} dx"), "∫₀¹ e⁻ˣ dx");
    }

    #[test]
    fn roots_fractions_accents() {
        assert_eq!(u(r"\sqrt{x}"), "√x");
        assert_eq!(u(r"\sqrt{x+1}"), "√(x+1)");
        assert_eq!(u(r"\sqrt[3]{8}"), "∛8");
        assert_eq!(u(r"\frac{a}{b}"), "a/b");
        assert_eq!(u(r"\frac{12}{35}"), "¹²⁄₃₅");
        assert_eq!(u(r"\frac{x+1}{2}"), "(x+1)/2");
        assert_eq!(u(r"\hat{x}"), "x\u{302}");
        assert_eq!(u(r"\vec v"), "v\u{20D7}");
        assert_eq!(u(r"\left( a \right)"), "( a )");
        assert_eq!(u(r"\{1, 2\}"), "{1, 2}");
        assert_eq!(u(r"\text{if } x > 0"), "if  x > 0");
        assert_eq!(u(r"\sin\theta"), "sinθ");
    }

    #[test]
    fn says_what_it_cannot_write() {
        assert_eq!(to_unicode(r"\frac{\frac{1}{2}}{3}"), Err(Error::Nested));
        assert_eq!(to_unicode(r"x^q"), Err(Error::NoSuperscript('q')));
        assert_eq!(to_unicode(r"\int_0^\infty"), Err(Error::NoSuperscript('∞')));
        assert_eq!(to_unicode(r"x_y"), Err(Error::NoSubscript('y')));
        assert_eq!(to_unicode(r"\foo"), Err(Error::Unknown("foo".into())));
        assert_eq!(
            to_unicode(r"\begin{pmatrix} 1 \end{pmatrix}"),
            Err(Error::Environment)
        );
        assert_eq!(to_unicode(r"x^"), Err(Error::Unbalanced));
        assert_eq!(to_unicode(r"\frac{1}{2"), Err(Error::Unbalanced));
    }

    #[test]
    fn what_looks_like_latex() {
        assert!(looks_like_latex(r"\alpha"));
        assert!(looks_like_latex("x^2"));
        assert!(looks_like_latex("a_n"));
        assert!(!looks_like_latex("arrow"));
        assert!(!looks_like_latex("snake_ case"));
        assert!(!looks_like_latex(r"C:\ "));
        assert!(!looks_like_latex("^_^"));
    }
}
