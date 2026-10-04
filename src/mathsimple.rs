//! Math written the short way (2.4), as Word's own equation editor reads
//! it (UnicodeMath): `x^2 + (2x)/8 + x^(3/2) + pi + Sigma_(i=1)^(n) X_i`.
//! Brackets group without braces or backslashes; names (`pi`, `sigma`,
//! `sqrt`) need no `\`; `Sigma` or `sum` with limits is the sum sign.
//!
//! Written three ways:
//! - [`Math::unicode`]: one line of Unicode (x² + 2x/8 + …), a part that
//!   has no Unicode form (x^(3/2)) left in the short form;
//! - [`Math::unicode_math`]: for an app's equation editor (Alt+= in Word,
//!   PowerPoint, OneNote), which builds it up into a real equation;
//! - [`Math::formatted`]: text with Word's superscript and subscript
//!   formatting, for the parts Unicode has no small letters for.
//!
//! No OS calls.

#[derive(Debug, Clone, PartialEq, Eq)]
enum Node {
    /// As written (in Unicode: π, ≤, x, 2, +, a space).
    Text(String),
    /// `( … )`, kept.
    Paren(Vec<Node>),
    Script {
        base: Box<Node>,
        sub: Option<Vec<Node>>,
        sup: Option<Vec<Node>>,
    },
    Frac(Vec<Node>, Vec<Node>),
    Sqrt(Vec<Node>),
}

/// Names written without a backslash. Not `xi` or `nu`: as often two
/// letters side by side.
const NAMES: &[(&str, &str)] = &[
    ("alpha", "α"),
    ("beta", "β"),
    ("gamma", "γ"),
    ("delta", "δ"),
    ("epsilon", "ε"),
    ("zeta", "ζ"),
    ("eta", "η"),
    ("theta", "θ"),
    ("iota", "ι"),
    ("kappa", "κ"),
    ("lambda", "λ"),
    ("mu", "μ"),
    ("pi", "π"),
    ("rho", "ρ"),
    ("sigma", "σ"),
    ("tau", "τ"),
    ("upsilon", "υ"),
    ("phi", "φ"),
    ("chi", "χ"),
    ("psi", "ψ"),
    ("omega", "ω"),
    ("Gamma", "Γ"),
    ("Delta", "Δ"),
    ("Theta", "Θ"),
    ("Lambda", "Λ"),
    ("Xi", "Ξ"),
    ("Pi", "Π"),
    ("Sigma", "Σ"),
    ("Phi", "Φ"),
    ("Psi", "Ψ"),
    ("Omega", "Ω"),
    ("inf", "∞"),
    ("infty", "∞"),
    ("infinity", "∞"),
    ("sum", "∑"),
    ("prod", "∏"),
    ("int", "∫"),
    ("oint", "∮"),
    ("partial", "∂"),
    ("nabla", "∇"),
    ("deg", "°"),
];

/// Written as they are: `sin x`.
const FUNCTIONS: &[&str] = &[
    "sin", "cos", "tan", "cot", "sec", "csc", "log", "ln", "exp", "lim", "max", "min", "det",
];

/// Two-character operators.
const OPERATORS: &[(&str, &str)] = &[
    ("<=", "≤"),
    (">=", "≥"),
    ("!=", "≠"),
    ("+-", "±"),
    ("-+", "∓"),
    ("->", "→"),
    ("=>", "⇒"),
    ("~=", "≈"),
    ("...", "…"),
];

struct Parser {
    chars: Vec<char>,
    at: usize,
    /// Something written short was read: a script, a fraction, a root, a
    /// name or an operator. A name alone is not counted (the character
    /// list has it).
    shaped: bool,
    named: bool,
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }

    fn rest_starts(&self, s: &str) -> bool {
        let n = s.chars().count();
        self.chars.len() >= self.at + n
            && self.chars[self.at..self.at + n]
                .iter()
                .copied()
                .eq(s.chars())
    }

    /// Items up to the end or `close` (taken).
    fn sequence(&mut self, close: Option<char>) -> Option<Vec<Node>> {
        let mut out = Vec::new();
        loop {
            match self.peek() {
                None if close.is_some() => return None,
                None => return Some(out),
                Some(c) if Some(c) == close => {
                    self.at += 1;
                    return Some(out);
                }
                Some(')') => return None,
                _ => {
                    let item = self.item()?;
                    out.push(item);
                }
            }
        }
    }

    /// A primary with its scripts, and a fraction it starts.
    fn item(&mut self) -> Option<Node> {
        let first = self.scripted()?;
        if matches!(first, Node::Text(ref t) if t == " ") {
            return Some(first);
        }
        // `a/b`, `(2x) / 8`.
        let save = self.at;
        while self.peek() == Some(' ') {
            self.at += 1;
        }
        if self.peek() == Some('/') {
            self.at += 1;
            while self.peek() == Some(' ') {
                self.at += 1;
            }
            if let Some(den) = self
                .scripted()
                .filter(|d| !matches!(d, Node::Text(t) if t == " "))
            {
                self.shaped = true;
                return Some(Node::Frac(ungroup(first), ungroup(den)));
            }
            return None;
        }
        self.at = save;
        Some(first)
    }

    fn scripted(&mut self) -> Option<Node> {
        let mut base = self.primary()?;
        if matches!(base, Node::Text(ref t) if t == " ") {
            return Some(base);
        }
        while let Some(c @ ('^' | '_')) = self.peek() {
            self.at += 1;
            let arg = self.script_arg()?;
            self.shaped = true;
            // `Sigma_` is the sum sign, not the letter.
            if let Node::Text(t) = &mut base {
                match t.as_str() {
                    "Σ" => *t = "∑".into(),
                    "Π" => *t = "∏".into(),
                    _ => {}
                }
            }
            base = match base {
                Node::Script { base, sub, sup } => match c {
                    '_' if sub.is_none() => Node::Script {
                        base,
                        sub: Some(arg),
                        sup,
                    },
                    '^' if sup.is_none() => Node::Script {
                        base,
                        sub,
                        sup: Some(arg),
                    },
                    _ => return None,
                },
                other => Node::Script {
                    base: Box::new(other),
                    sub: (c == '_').then(|| arg.clone()),
                    sup: (c == '^').then_some(arg),
                },
            };
        }
        Some(base)
    }

    /// What `^` or `_` takes: `(…)`, a number, a sign and what follows, a
    /// name, or one character.
    fn script_arg(&mut self) -> Option<Vec<Node>> {
        match self.peek()? {
            '(' => {
                self.at += 1;
                self.sequence(Some(')'))
            }
            '+' | '-' => {
                let sign = self.peek()?;
                self.at += 1;
                let mut v = vec![Node::Text(sign.to_string())];
                v.extend(self.script_arg()?);
                Some(v)
            }
            c if c.is_ascii_digit() => Some(vec![Node::Text(self.number())]),
            c if c.is_ascii_alphabetic() => {
                let run = self.letters_ahead();
                if let Some((_, sym)) = NAMES.iter().find(|(n, _)| *n == run) {
                    self.at += run.chars().count();
                    return Some(vec![Node::Text(sym.to_string())]);
                }
                self.at += 1;
                Some(vec![Node::Text(c.to_string())])
            }
            ' ' => None,
            c => {
                self.at += 1;
                Some(vec![Node::Text(c.to_string())])
            }
        }
    }

    fn number(&mut self) -> String {
        let mut s = String::new();
        while let Some(c) = self.peek() {
            let point = c == '.'
                && self
                    .chars
                    .get(self.at + 1)
                    .is_some_and(|n| n.is_ascii_digit())
                && !s.is_empty();
            if c.is_ascii_digit() || point {
                s.push(c);
                self.at += 1;
            } else {
                break;
            }
        }
        s
    }

    fn letters_ahead(&self) -> String {
        self.chars[self.at..]
            .iter()
            .take_while(|c| c.is_ascii_alphabetic())
            .collect()
    }

    fn primary(&mut self) -> Option<Node> {
        let c = self.peek()?;
        if c == ' ' {
            while self.peek() == Some(' ') {
                self.at += 1;
            }
            return Some(Node::Text(" ".into()));
        }
        if let Some((op, sym)) = OPERATORS.iter().find(|(op, _)| self.rest_starts(op)) {
            self.at += op.chars().count();
            self.shaped = true;
            return Some(Node::Text(sym.to_string()));
        }
        match c {
            '(' => {
                self.at += 1;
                Some(Node::Paren(self.sequence(Some(')'))?))
            }
            '{' | '}' | '\\' | '^' | '_' | '/' | ')' => None,
            '*' => {
                self.at += 1;
                self.shaped = true;
                Some(Node::Text("·".into()))
            }
            c if c.is_ascii_digit() => Some(Node::Text(self.number())),
            c if c.is_ascii_alphabetic() => {
                let run = self.letters_ahead();
                self.at += run.chars().count();
                if run == "sqrt" || run == "cbrt" {
                    while self.peek() == Some(' ') {
                        self.at += 1;
                    }
                    let arg = ungroup(self.scripted()?);
                    self.shaped = true;
                    return Some(if run == "sqrt" {
                        Node::Sqrt(arg)
                    } else {
                        Node::Text(format!("∛{}", linear(&arg)))
                    });
                }
                if let Some((_, sym)) = NAMES.iter().find(|(n, _)| *n == run) {
                    self.named = true;
                    return Some(Node::Text(sym.to_string()));
                }
                if FUNCTIONS.contains(&run.as_str()) || run.chars().count() <= 3 {
                    return Some(Node::Text(run));
                }
                None
            }
            c if c.is_ascii() || "×÷±≤≥≠≈→∞°·−".contains(c) => {
                self.at += 1;
                Some(Node::Text(c.to_string()))
            }
            // Thai, and anything else: not math written short.
            _ => None,
        }
    }
}

/// `(x+1)` as the inside of a fraction or a root: its brackets dropped.
fn ungroup(n: Node) -> Vec<Node> {
    match n {
        Node::Paren(inner) => inner,
        other => vec![other],
    }
}

fn is_single(v: &[Node]) -> bool {
    match v {
        [Node::Text(t)] => t.chars().count() == 1 || t.chars().all(|c| c.is_ascii_digit()),
        _ => false,
    }
}

/// The short form again: the way it was written, tidied (the form Word's
/// equation editor builds up).
fn linear(v: &[Node]) -> String {
    v.iter().map(linear_one).collect()
}

fn wrapped(v: &[Node]) -> String {
    if is_single(v) {
        linear(v)
    } else {
        format!("({})", linear(v))
    }
}

fn linear_one(n: &Node) -> String {
    match n {
        Node::Text(t) => t.clone(),
        Node::Paren(v) => format!("({})", linear(v)),
        Node::Script { base, sub, sup } => {
            let mut s = linear_one(base);
            if let Some(sub) = sub {
                s.push('_');
                s.push_str(&wrapped(sub));
            }
            if let Some(sup) = sup {
                s.push('^');
                s.push_str(&wrapped(sup));
            }
            s
        }
        Node::Frac(a, b) => format!("{}/{}", wrapped(a), wrapped(b)),
        Node::Sqrt(v) => format!("√{}", wrapped(v)),
    }
}

/// As LaTeX for [`crate::latex::to_unicode`] (symbols already Unicode).
fn latex(v: &[Node]) -> String {
    v.iter().map(latex_one).collect()
}

fn latex_one(n: &Node) -> String {
    match n {
        Node::Text(t) => t.clone(),
        Node::Paren(v) => format!("({})", latex(v)),
        Node::Script { base, sub, sup } => {
            let mut s = format!("{{{}}}", latex_one(base));
            if let Some(sub) = sub {
                s.push_str(&format!("_{{{}}}", latex(sub)));
            }
            if let Some(sup) = sup {
                s.push_str(&format!("^{{{}}}", latex(sup)));
            }
            s
        }
        Node::Frac(a, b) => format!(r"\frac{{{}}}{{{}}}", latex(a), latex(b)),
        Node::Sqrt(v) => format!(r"\sqrt{{{}}}", latex(v)),
    }
}

/// One piece of [`Math::formatted`] text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Piece {
    Text(String),
    Sup(String),
    Sub(String),
}

fn push(out: &mut Vec<Piece>, p: Piece) {
    let merged = match (out.last_mut(), &p) {
        (Some(Piece::Text(a)), Piece::Text(b))
        | (Some(Piece::Sup(a)), Piece::Sup(b))
        | (Some(Piece::Sub(a)), Piece::Sub(b)) => {
            a.push_str(b);
            true
        }
        _ => false,
    };
    if !merged && !matches!(&p, Piece::Text(t) | Piece::Sup(t) | Piece::Sub(t) if t.is_empty()) {
        out.push(p);
    }
}

fn formatted(v: &[Node], out: &mut Vec<Piece>) {
    for n in v {
        match n {
            Node::Text(t) => push(out, Piece::Text(t.clone())),
            Node::Paren(inner) => {
                push(out, Piece::Text("(".into()));
                formatted(inner, out);
                push(out, Piece::Text(")".into()));
            }
            Node::Script { base, sub, sup } => {
                formatted(std::slice::from_ref(base), out);
                if let Some(sub) = sub {
                    push(out, Piece::Sub(linear(sub)));
                }
                if let Some(sup) = sup {
                    push(out, Piece::Sup(linear(sup)));
                }
            }
            Node::Frac(a, b) => {
                // A number over a number as a small fraction: ³⁄₂.
                if is_single(a)
                    && is_single(b)
                    && linear(a).chars().all(|c| c.is_ascii_digit())
                    && linear(b).chars().all(|c| c.is_ascii_digit())
                {
                    push(out, Piece::Sup(linear(a)));
                    push(out, Piece::Text("⁄".into()));
                    push(out, Piece::Sub(linear(b)));
                } else {
                    let side = |v: &Vec<Node>, out: &mut Vec<Piece>| {
                        if is_single(v) {
                            formatted(v, out);
                        } else {
                            push(out, Piece::Text("(".into()));
                            formatted(v, out);
                            push(out, Piece::Text(")".into()));
                        }
                    };
                    side(a, out);
                    push(out, Piece::Text("/".into()));
                    side(b, out);
                }
            }
            Node::Sqrt(inner) => {
                push(out, Piece::Text("√".into()));
                if is_single(inner) {
                    formatted(inner, out);
                } else {
                    push(out, Piece::Text("(".into()));
                    formatted(inner, out);
                    push(out, Piece::Text(")".into()));
                }
            }
        }
    }
}

/// Math written the short way, read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Math {
    nodes: Vec<Node>,
}

impl Math {
    /// `src` read, if it is math written short: something shaped (a script,
    /// a fraction, a root, `<=`), every word a name, a function or a short
    /// variable. A name alone (`pi`) is the character list's.
    pub fn read(src: &str) -> Option<Math> {
        let src = src.trim();
        if src.is_empty() {
            return None;
        }
        let mut p = Parser {
            chars: src.chars().collect(),
            at: 0,
            shaped: false,
            named: false,
        };
        let nodes = p.sequence(None)?;
        let lone_name = p.named && !p.shaped && nodes.len() == 1;
        if !(p.shaped || p.named) || lone_name {
            return None;
        }
        Some(Math { nodes })
    }

    /// One line of Unicode. A part with no Unicode form stays short
    /// (`x^(3/2)`).
    pub fn unicode(&self) -> String {
        self.nodes
            .iter()
            .map(|n| match n {
                Node::Text(t) => t.clone(),
                other => crate::latex::to_unicode(&latex_one(other))
                    .unwrap_or_else(|_| linear_one(other)),
            })
            .collect()
    }

    /// Whether [`Math::unicode`] wrote every part (nothing left short).
    pub fn fully_unicode(&self) -> bool {
        self.nodes.iter().all(|n| match n {
            Node::Text(_) => true,
            other => crate::latex::to_unicode(&latex_one(other)).is_ok(),
        })
    }

    /// For an equation editor (UnicodeMath): `x^(3/2)+π+∑_(i=1)^n X_i`.
    pub fn unicode_math(&self) -> String {
        linear(&self.nodes)
    }

    /// Text with superscript and subscript parts, for an app's formatting.
    pub fn formatted(&self) -> Vec<Piece> {
        let mut out = Vec::new();
        formatted(&self.nodes, &mut out);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWNER: &str = "x^2 + (2x)/8 + x/2 +x^(3/2) +pi + sigma + Sigma_(i=1)^(n) X_i";

    #[test]
    fn the_owners_example() {
        let m = Math::read(OWNER).unwrap();
        assert_eq!(m.unicode(), "x² + 2x/8 + x/2 +x^(3/2) +π + σ + ∑ᵢ₌₁ⁿ Xᵢ");
        assert!(!m.fully_unicode());
        assert_eq!(
            m.unicode_math(),
            "x^2 + (2x)/8 + x/2 +x^(3/2) +π + σ + ∑_(i=1)^n X_i"
        );
        let f = m.formatted();
        assert_eq!(f[0], Piece::Text("x".into()));
        assert_eq!(f[1], Piece::Sup("2".into()));
        assert!(f.contains(&Piece::Sup("3/2".into())));
        assert!(f.contains(&Piece::Sub("i=1".into())));
    }

    #[test]
    fn short_forms() {
        let u = |s: &str| Math::read(s).map(|m| m.unicode());
        assert_eq!(u("x^10 + 1").as_deref(), Some("x¹⁰ + 1"));
        assert_eq!(u("1/2").as_deref(), Some("½"));
        assert_eq!(u("(x+1)/(x-1)").as_deref(), Some("(x+1)/(x−1)"));
        assert_eq!(u("sqrt(x+1)").as_deref(), Some("√(x+1)"));
        assert_eq!(u("H_2O").as_deref(), Some("H₂O"));
        assert_eq!(u("e^-x").as_deref(), Some("e⁻ˣ"));
        assert_eq!(u("a <= b != c").as_deref(), Some("a ≤ b ≠ c"));
        assert_eq!(u("2 pi r").as_deref(), Some("2 π r"));
        assert_eq!(u("x_n^2").as_deref(), Some("xₙ²"));
    }

    #[test]
    fn not_math() {
        for s in [
            "pi",
            "hello world",
            "and/or",
            "TCP/IP",
            "ไปกินข้าว",
            "x^",
            "(x+1",
            "x^{2}",
            r"\alpha",
            "",
            "plain words here",
        ] {
            let got = Math::read(s).map(|m| m.unicode());
            assert!(got.is_none() || got.as_deref() == Some(s), "{s}: {got:?}");
        }
    }
}
