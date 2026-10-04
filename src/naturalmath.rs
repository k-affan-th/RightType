//! Math said in words (2.4), for people who don't know LaTeX: "x squared
//! plus y squared", "รากที่สองของ x", "a ส่วน b", "sum from i=1 to n" —
//! read into LaTeX, then into one line of Unicode by [`crate::latex`].
//!
//! Thai has no spaces between words, so phrases are found by matching at
//! each position, longest first; English phrases must end at a word's end.
//! Anything that is not a known phrase, a short variable (`x`, `ab`), a
//! number or a function name (`sin`) means the text is not math, and
//! nothing is offered. No OS calls.

use std::sync::OnceLock;

/// What a phrase does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Act {
    /// Between two operands: `+`, `\le`.
    Op(&'static str),
    /// An operand on its own: `\pi`.
    Atom(&'static str),
    /// Added to the operand before it: `^{2}`, `!`.
    Post(&'static str),
    /// The next operand is the power of the one before (`to the power of`).
    PowerOf,
    /// The next operand is the subscript of the one before (`sub`).
    Sub,
    /// The next operand goes under a root: `\sqrt`, `\sqrt[3]`.
    Root(&'static str),
    /// The operand before over the next one.
    Over,
    /// A big operator, with an optional `from … to …`: `\sum`.
    Big(&'static str),
    From,
    To,
    /// Said, but adds nothing (`of`, `the`, `เศษ`).
    Filler,
}

use Act::*;

const PHRASES: &[(&str, Act)] = &[
    // Powers and roots.
    ("squared", Post("^{2}")),
    ("cubed", Post("^{3}")),
    ("to the power of", PowerOf),
    ("to the power", PowerOf),
    ("raised to the power of", PowerOf),
    ("raised to", PowerOf),
    ("power", PowerOf),
    ("ยกกำลัง", PowerOf),
    ("กำลัง", PowerOf),
    ("square root of", Root(r"\sqrt")),
    ("square root", Root(r"\sqrt")),
    ("root of", Root(r"\sqrt")),
    ("sqrt", Root(r"\sqrt")),
    ("cube root of", Root(r"\sqrt[3]")),
    ("cube root", Root(r"\sqrt[3]")),
    ("รากที่สองของ", Root(r"\sqrt")),
    ("รากที่สอง", Root(r"\sqrt")),
    ("รากที่2ของ", Root(r"\sqrt")),
    ("รากที่ 2 ของ", Root(r"\sqrt")),
    ("รากที่สามของ", Root(r"\sqrt[3]")),
    ("รากที่สาม", Root(r"\sqrt[3]")),
    ("รากที่3ของ", Root(r"\sqrt[3]")),
    ("รากที่ 3 ของ", Root(r"\sqrt[3]")),
    ("รากที่สี่ของ", Root(r"\sqrt[4]")),
    ("รากของ", Root(r"\sqrt")),
    ("ราก", Root(r"\sqrt")),
    ("sub", Sub),
    ("ห้อย", Sub),
    // Arithmetic.
    ("plus or minus", Op(r"\pm")),
    ("plus minus", Op(r"\pm")),
    ("บวกหรือลบ", Op(r"\pm")),
    ("บวกลบ", Op(r"\pm")),
    ("plus", Op("+")),
    ("บวก", Op("+")),
    ("minus", Op("-")),
    ("ลบ", Op("-")),
    ("times", Op(r"\times")),
    ("multiplied by", Op(r"\times")),
    ("คูณด้วย", Op(r"\times")),
    ("คูณ", Op(r"\times")),
    ("divided by", Op(r"\div")),
    ("หารด้วย", Op(r"\div")),
    ("หาร", Op(r"\div")),
    ("over", Over),
    ("ส่วน", Over),
    ("เศษ", Filler),
    // Comparing.
    ("is less than or equal to", Op(r"\le")),
    ("less than or equal to", Op(r"\le")),
    ("น้อยกว่าหรือเท่ากับ", Op(r"\le")),
    ("is greater than or equal to", Op(r"\ge")),
    ("greater than or equal to", Op(r"\ge")),
    ("มากกว่าหรือเท่ากับ", Op(r"\ge")),
    ("is less than", Op("<")),
    ("less than", Op("<")),
    ("น้อยกว่า", Op("<")),
    ("is greater than", Op(">")),
    ("greater than", Op(">")),
    ("มากกว่า", Op(">")),
    ("is not equal to", Op(r"\neq")),
    ("not equal to", Op(r"\neq")),
    ("does not equal", Op(r"\neq")),
    ("not equals", Op(r"\neq")),
    ("ไม่เท่ากับ", Op(r"\neq")),
    ("is approximately equal to", Op(r"\approx")),
    ("approximately equal to", Op(r"\approx")),
    ("is approximately", Op(r"\approx")),
    ("approximately", Op(r"\approx")),
    ("ประมาณเท่ากับ", Op(r"\approx")),
    ("มีค่าประมาณ", Op(r"\approx")),
    ("ประมาณ", Op(r"\approx")),
    ("is equal to", Op("=")),
    ("equal to", Op("=")),
    ("equals", Op("=")),
    ("มีค่าเท่ากับ", Op("=")),
    ("เท่ากับ", Op("=")),
    ("is an element of", Op(r"\in")),
    ("is in", Op(r"\in")),
    ("element of", Op(r"\in")),
    ("เป็นสมาชิกของ", Op(r"\in")),
    ("implies", Op(r"\Rightarrow")),
    // Sums and integrals.
    ("the sum of", Big(r"\sum")),
    ("sum of", Big(r"\sum")),
    ("sum", Big(r"\sum")),
    ("summation", Big(r"\sum")),
    ("ผลรวมของ", Big(r"\sum")),
    ("ผลรวม", Big(r"\sum")),
    ("the integral of", Big(r"\int")),
    ("integral of", Big(r"\int")),
    ("integral", Big(r"\int")),
    ("อินทิกรัลของ", Big(r"\int")),
    ("อินทิกรัล", Big(r"\int")),
    ("ปริพันธ์ของ", Big(r"\int")),
    ("ปริพันธ์", Big(r"\int")),
    ("product of", Big(r"\prod")),
    ("ผลคูณของ", Big(r"\prod")),
    ("from", From),
    ("จาก", From),
    ("ตั้งแต่", From),
    ("to", To),
    ("จนถึง", To),
    ("ถึง", To),
    // Symbols.
    ("infinity", Atom(r"\infty")),
    ("อนันต์", Atom(r"\infty")),
    ("for all", Atom(r"\forall")),
    ("for every", Atom(r"\forall")),
    ("สำหรับทุก", Atom(r"\forall")),
    ("there exists", Atom(r"\exists")),
    ("มี", Atom(r"\exists")),
    ("degrees", Post("°")),
    ("degree", Post("°")),
    ("องศา", Post("°")),
    ("factorial", Post("!")),
    ("แฟกทอเรียล", Post("!")),
    ("percent", Post("%")),
    ("เปอร์เซ็นต์", Post("%")),
    ("prime", Post("'")),
    ("ไพรม์", Post("'")),
    ("pi", Atom(r"\pi")),
    ("พาย", Atom(r"\pi")),
    ("alpha", Atom(r"\alpha")),
    ("อัลฟา", Atom(r"\alpha")),
    ("อัลฟ่า", Atom(r"\alpha")),
    ("beta", Atom(r"\beta")),
    ("เบตา", Atom(r"\beta")),
    ("เบต้า", Atom(r"\beta")),
    ("gamma", Atom(r"\gamma")),
    ("แกมมา", Atom(r"\gamma")),
    ("แกมม่า", Atom(r"\gamma")),
    ("delta", Atom(r"\Delta")),
    ("เดลตา", Atom(r"\Delta")),
    ("เดลต้า", Atom(r"\Delta")),
    ("epsilon", Atom(r"\varepsilon")),
    ("เอปซิลอน", Atom(r"\varepsilon")),
    ("theta", Atom(r"\theta")),
    ("ทีตา", Atom(r"\theta")),
    ("ธีตา", Atom(r"\theta")),
    ("ทีต้า", Atom(r"\theta")),
    ("lambda", Atom(r"\lambda")),
    ("แลมบ์ดา", Atom(r"\lambda")),
    ("แลมดา", Atom(r"\lambda")),
    ("mu", Atom(r"\mu")),
    ("มิว", Atom(r"\mu")),
    ("sigma", Atom(r"\sigma")),
    ("ซิกมา", Atom(r"\sigma")),
    ("ซิกม่า", Atom(r"\sigma")),
    ("omega", Atom(r"\omega")),
    ("โอเมกา", Atom(r"\omega")),
    ("โอเมก้า", Atom(r"\omega")),
    ("phi", Atom(r"\phi")),
    ("ฟาย", Atom(r"\phi")),
    ("rho", Atom(r"\rho")),
    ("tau", Atom(r"\tau")),
    // Numbers said as words.
    ("zero", Atom("0")),
    ("one", Atom("1")),
    ("two", Atom("2")),
    ("three", Atom("3")),
    ("four", Atom("4")),
    ("five", Atom("5")),
    ("six", Atom("6")),
    ("seven", Atom("7")),
    ("eight", Atom("8")),
    ("nine", Atom("9")),
    ("ten", Atom("10")),
    ("ศูนย์", Atom("0")),
    ("หนึ่ง", Atom("1")),
    ("สอง", Atom("2")),
    ("สาม", Atom("3")),
    ("สี่", Atom("4")),
    ("ห้า", Atom("5")),
    ("หก", Atom("6")),
    ("เจ็ด", Atom("7")),
    ("แปด", Atom("8")),
    ("เก้า", Atom("9")),
    ("สิบ", Atom("10")),
    // Said, adding nothing.
    ("of", Filler),
    ("the", Filler),
    ("is", Filler),
    ("ของ", Filler),
];

/// Function names written as they are: `sin x`.
const FUNCTIONS: &[&str] = &[
    "sin", "cos", "tan", "cot", "sec", "csc", "log", "ln", "exp", "lim", "max", "min",
];

/// The phrases, longest first, so `บวกลบ` wins over `บวก`.
fn phrases() -> &'static [(&'static str, Act)] {
    static SORTED: OnceLock<Vec<(&str, Act)>> = OnceLock::new();
    SORTED.get_or_init(|| {
        let mut v = PHRASES.to_vec();
        v.sort_by_key(|(p, _)| std::cmp::Reverse(p.chars().count()));
        v
    })
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Act(Act),
    /// A variable, number or function, as LaTeX.
    Text(String),
    Open,
    Close,
}

/// Does `rest` start with `phrase`, ignoring ASCII case, ending where a word
/// does (for a phrase ending in a Latin letter)? Spaces in the phrase match
/// any run of spaces, or none between Thai words.
fn starts_with_phrase(rest: &str, phrase: &str) -> Option<usize> {
    let mut r = rest.char_indices().peekable();
    let mut used = 0;
    let mut p = phrase.chars().peekable();
    while let Some(pc) = p.next() {
        if pc == ' ' {
            let mut any = false;
            while r.peek().is_some_and(|(_, c)| *c == ' ') {
                r.next();
                any = true;
            }
            let thai_next = p.peek().is_some_and(|c| !c.is_ascii());
            if !any && !thai_next {
                return None;
            }
            continue;
        }
        let (i, rc) = r.next()?;
        if !rc.eq_ignore_ascii_case(&pc) {
            return None;
        }
        used = i + rc.len_utf8();
    }
    let last = phrase.chars().last()?;
    if last.is_ascii_alphabetic()
        && rest[used..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic())
    {
        return None;
    }
    Some(used)
}

/// The tokens, and how many were said in words (not fillers).
fn tokens(text: &str) -> Option<(Vec<Tok>, usize)> {
    let mut out = Vec::new();
    let mut said = 0;
    let mut i = 0;
    while i < text.len() {
        let rest = &text[i..];
        let c = rest.chars().next()?;
        if c.is_whitespace() {
            i += c.len_utf8();
            continue;
        }
        // A word starts here (Latin) or anywhere (Thai).
        let word_start = !c.is_ascii_alphabetic()
            || !text[..i]
                .chars()
                .next_back()
                .is_some_and(|p| p.is_ascii_alphanumeric());
        if word_start {
            if let Some((len, act)) = phrases()
                .iter()
                .find_map(|(p, a)| starts_with_phrase(rest, p).map(|n| (n, *a)))
            {
                out.push(Tok::Act(act));
                said += usize::from(act != Filler);
                i += len;
                continue;
            }
        }
        match c {
            '(' | '[' => out.push(Tok::Open),
            ')' | ']' => out.push(Tok::Close),
            '+' => out.push(Tok::Act(Op("+"))),
            '-' | '−' => out.push(Tok::Act(Op("-"))),
            '*' | '×' => out.push(Tok::Act(Op(r"\times"))),
            '÷' => out.push(Tok::Act(Op(r"\div"))),
            '/' => out.push(Tok::Act(Over)),
            '=' => out.push(Tok::Act(Op("="))),
            '<' => out.push(Tok::Act(Op("<"))),
            '>' => out.push(Tok::Act(Op(">"))),
            '^' => out.push(Tok::Act(PowerOf)),
            '_' => out.push(Tok::Act(Sub)),
            '!' => out.push(Tok::Act(Post("!"))),
            ',' => out.push(Tok::Text(",".into())),
            c if c.is_ascii_alphanumeric() => {
                let run: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '.')
                    .collect();
                let run = run.trim_end_matches('.').to_string();
                let letters = run.chars().filter(|c| c.is_ascii_alphabetic()).count();
                let lower = run.to_ascii_lowercase();
                if FUNCTIONS.contains(&lower.as_str()) {
                    out.push(Tok::Text(format!(r"\{lower} ")));
                } else if letters <= 2 {
                    out.push(Tok::Text(run.clone()));
                } else {
                    return None;
                }
                i += run.len();
                continue;
            }
            _ => return None,
        }
        i += c.len_utf8();
    }
    // Number words side by side (`สามสิบ`, `twenty one`) are one number
    // said in parts, not two numbers: rather nothing than 310.
    let number = |t: &Tok| matches!(t, Tok::Act(Atom(a)) if a.chars().all(|c| c.is_ascii_digit()));
    if out.windows(2).any(|w| number(&w[0]) && number(&w[1])) {
        return None;
    }
    Some((out, said))
}

struct Reader {
    toks: Vec<Tok>,
    at: usize,
}

impl Reader {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.at)
    }

    fn skip_fillers(&mut self) {
        while self.peek() == Some(&Tok::Act(Filler)) {
            self.at += 1;
        }
    }

    /// Operands and operators up to the end, a `)`, or (inside a sum's
    /// `from`) a `to`.
    fn sequence(&mut self, until_to: bool) -> Option<String> {
        let mut out = String::new();
        let mut last_was_op = true;
        loop {
            self.skip_fillers();
            match self.peek() {
                None | Some(Tok::Close) => break,
                Some(Tok::Act(To)) if until_to => break,
                Some(Tok::Act(Op(op))) => {
                    let op = *op;
                    self.at += 1;
                    out.push_str(&format!(" {op} "));
                    last_was_op = true;
                }
                _ => {
                    let operand = self.operand()?;
                    // `sin x`, `2 x`: side by side, a space only after a word.
                    if !last_was_op
                        && out.ends_with(|c: char| c.is_ascii_alphabetic())
                        && operand.starts_with(|c: char| c.is_ascii_alphabetic())
                    {
                        out.push(' ');
                    }
                    out.push_str(&operand);
                    last_was_op = false;
                }
            }
        }
        Some(out.trim().to_string())
    }

    /// One operand and what follows it: `x squared`, `a over b`.
    fn operand(&mut self) -> Option<String> {
        let mut base = self.primary()?;
        loop {
            self.skip_fillers();
            match self.peek() {
                Some(Tok::Act(Post(p))) => {
                    base = format!("{{{base}}}{p}");
                    self.at += 1;
                }
                Some(Tok::Act(PowerOf)) => {
                    self.at += 1;
                    let e = self.primary()?;
                    base = format!("{{{base}}}^{{{e}}}");
                }
                Some(Tok::Act(Sub)) => {
                    self.at += 1;
                    let e = self.primary()?;
                    base = format!("{{{base}}}_{{{e}}}");
                }
                Some(Tok::Act(Over)) => {
                    self.at += 1;
                    let den = self.operand()?;
                    base = format!(r"\frac{{{}}}{{{den}}}", strip_parens(&base));
                }
                _ => return Some(base),
            }
        }
    }

    fn primary(&mut self) -> Option<String> {
        self.skip_fillers();
        let tok = self.peek()?.clone();
        self.at += 1;
        match tok {
            Tok::Text(t) => Some(t),
            Tok::Act(Atom(a)) => Some(format!("{{{a}}}")),
            Tok::Open => {
                let inner = self.sequence(false)?;
                if self.peek() != Some(&Tok::Close) {
                    return None;
                }
                self.at += 1;
                Some(format!("({inner})"))
            }
            Tok::Act(Root(r)) => {
                let arg = self.operand()?;
                Some(format!("{r}{{{}}}", strip_parens(&arg)))
            }
            Tok::Act(Big(b)) => {
                self.skip_fillers();
                let mut out = b.to_string();
                if self.peek() == Some(&Tok::Act(From)) {
                    self.at += 1;
                    let lo = self.sequence(true)?;
                    if self.peek() != Some(&Tok::Act(To)) {
                        return None;
                    }
                    self.at += 1;
                    let hi = self.primary()?;
                    out = format!("{out}_{{{}}}^{{{hi}}}", lo.replace(' ', ""));
                }
                // What is summed: the operand after, if any.
                self.skip_fillers();
                match self.peek() {
                    None | Some(Tok::Close) | Some(Tok::Act(Op(_))) => Some(out),
                    _ => Some(format!("{out} {}", self.operand()?)),
                }
            }
            _ => None,
        }
    }
}

/// `(x+1)` → `x+1`, for a root or fraction that adds its own brackets.
fn strip_parens(s: &str) -> &str {
    s.strip_prefix('(')
        .and_then(|s| s.strip_suffix(')'))
        .unwrap_or(s)
}

/// `text` said in words, as LaTeX: `x squared plus 1` → `{x}^{2} + 1`.
/// None unless at least one phrase was used and every word is understood.
pub fn to_latex(text: &str) -> Option<String> {
    let (toks, said) = tokens(text.trim())?;
    // Nothing said in words (`x+1`), or one word alone (`pi`, already in
    // the character list).
    if said == 0 || toks.len() < 2 {
        return None;
    }
    let mut r = Reader { toks, at: 0 };
    let out = r.sequence(false)?;
    (r.at == r.toks.len() && !out.is_empty()).then_some(out)
}

/// For a hint while typing (2.4): math said in words that the text before
/// the caret ends with, from its start or after a space — the longest —
/// as (characters it replaces, Unicode). Stricter than the list at the
/// cursor, where asking is explicit: something is done (a power, an
/// operator, a root), with a variable or number written as such (`x`, `1`,
/// not `สอง` alone), and nothing left hanging (`x บวก`).
pub fn offer(before: &str) -> Option<(usize, String)> {
    if before.ends_with(char::is_whitespace) {
        return None;
    }
    let starts = std::iter::once(0).chain(
        before
            .char_indices()
            .filter(|(_, c)| c.is_whitespace())
            .map(|(i, c)| i + c.len_utf8()),
    );
    for start in starts {
        let part = &before[start..];
        let Some((toks, _)) = tokens(part) else {
            continue;
        };
        let does = toks.iter().any(|t| {
            matches!(
                t,
                Tok::Act(Op(_) | Post(_) | PowerOf | Sub | Root(_) | Over | Big(_))
            )
        });
        let written = toks
            .iter()
            .any(|t| matches!(t, Tok::Text(s) if s.chars().any(|c| c.is_ascii_alphanumeric())));
        let hanging = matches!(
            toks.last(),
            Some(Tok::Act(
                Op(_) | PowerOf | Sub | Root(_) | Over | Big(_) | From | To
            ))
        );
        // Begins with what is worked on: `x`, `2`, `รากที่สองของ`, `(` —
        // not `ประมาณ 6` or `เท่ากับ 2` in a sentence.
        let begins = matches!(
            toks.first(),
            Some(Tok::Text(_) | Tok::Open | Tok::Act(Atom(_) | Root(_) | Big(_)))
        );
        // An operator between two things (not `(plus)`, `plus plus`), and a
        // sum or integral with its limits.
        let shaped = toks.windows(2).all(|w| match w {
            [Tok::Open | Tok::Act(Op(_)), Tok::Act(Op(_))] => false,
            [Tok::Act(Op(_)), Tok::Close] => false,
            [Tok::Act(Big(_)), next] => *next == Tok::Act(From),
            _ => true,
        }) && !matches!(toks.last(), Some(Tok::Act(Big(_))));
        if !does || !written || hanging || !begins || !shaped {
            continue;
        }
        if let Some(text) = to_unicode(part) {
            return Some((part.chars().count(), text));
        }
    }
    None
}

/// `text` said in words, as one line of Unicode: `x squared plus y squared`
/// → `x² + y²`.
pub fn to_unicode(text: &str) -> Option<String> {
    let latex = to_latex(text)?;
    let out = crate::latex::to_unicode(&latex).ok()?;
    (out != text.trim()).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(s: &str) -> String {
        to_unicode(s).unwrap_or_else(|| panic!("{s}: {:?}", to_latex(s)))
    }

    #[test]
    fn english() {
        assert_eq!(u("x squared plus y squared"), "x² + y²");
        assert_eq!(u("E equals m c squared"), "E = mc²");
        assert_eq!(u("square root of x"), "√x");
        assert_eq!(u("the square root of (x plus 1)"), "√(x + 1)");
        assert_eq!(u("cube root of 8"), "∛8");
        assert_eq!(u("a over b"), "a/b");
        assert_eq!(u("1 over 2"), "½");
        assert_eq!(u("x to the power of n"), "xⁿ");
        assert_eq!(u("x less than or equal to 5"), "x ≤ 5");
        assert_eq!(u("2 pi r"), "2πr");
        assert_eq!(u("sum from i=1 to n of x sub i"), "∑ᵢ₌₁ⁿ xᵢ");
        assert_eq!(u("integral from 0 to 1 of x dx"), "∫₀¹ x dx");
        assert_eq!(u("a plus or minus b"), "a ± b");
        assert_eq!(u("sin x squared"), "sin x²");
        assert_eq!(u("90 degrees"), "90°");
        assert_eq!(u("x approximately 3.14"), "x ≈ 3.14");
        assert_eq!(u("x not equal to infinity"), "x ≠ ∞");
        assert_eq!(u("x to the power of two"), "x²");
    }

    #[test]
    fn thai() {
        assert_eq!(u("x ยกกำลังสอง บวก y ยกกำลังสอง"), "x² + y²");
        assert_eq!(u("xยกกำลังสองบวกyยกกำลังสอง"), "x² + y²");
        assert_eq!(u("รากที่สองของ x"), "√x");
        assert_eq!(u("รากที่สามของ 27"), "∛27");
        assert_eq!(u("a ส่วน b"), "a/b");
        assert_eq!(u("เศษ 1 ส่วน 2"), "½");
        assert_eq!(u("x น้อยกว่าหรือเท่ากับ 5"), "x ≤ 5");
        assert_eq!(u("x มากกว่า 2"), "x > 2");
        assert_eq!(u("2 พาย r"), "2πr");
        assert_eq!(u("a บวกลบ b"), "a ± b");
        assert_eq!(u("ผลรวมจาก i=1 ถึง n ของ x ห้อย i"), "∑ᵢ₌₁ⁿ xᵢ");
        assert_eq!(u("x ยกกำลัง 10"), "x¹⁰");
        assert_eq!(u("x ไม่เท่ากับ อนันต์"), "x ≠ ∞");
        assert_eq!(u("90 องศา"), "90°");
        assert_eq!(u("รากที่สองของ x บวก 1 ส่วน 2"), "√x + ½");
    }

    #[test]
    fn offered_while_typing() {
        assert_eq!(offer("x ยกกำลังสองบวก 1"), Some((17, "x² + 1".to_string())));
        assert_eq!(offer("ผลลัพธ์คือ x ยกกำลังสอง"), Some((12, "x²".to_string())));
        assert_eq!(offer("a over b"), Some((8, "a/b".to_string())));
        for no in [
            "x ยกกำลังสองบวก",
            "x ยกกำลังสองบวก 1 ",
            "สองบวกสาม",
            "มีสามคน",
            "one plus one",
            "I have a plus side",
            "ราคา 5 บาท",
            "รากสามสิบ x",
            "times (times) t",
            "sum (sum) s",
        ] {
            assert_eq!(offer(no), None, "{no}");
        }
    }

    #[test]
    fn not_math() {
        for s in [
            "รากที่สองของ สามสิบ",
            "สามสิบ บวก x",
            "hello world",
            "pi",
            "alpha",
            "x+1",
            "from here to there",
            "สวัสดีครับ",
            "ไปกินข้าว",
            "squared",
            "",
            "the cat",
        ] {
            assert_eq!(to_unicode(s), None, "{s}");
        }
    }
}
