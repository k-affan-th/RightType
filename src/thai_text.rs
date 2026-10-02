//! Rewrites of finished Thai text, for the palette's selection commands:
//! putting Thai text into its standard form, swapping Buddhist and Common
//! Era years, and writing a number out in words (or as a baht amount).
//!
//! Plain functions of the text: no OS calls, nothing kept.

/// Thai text in its standard (searchable) form. Text copied out of PDFs and
/// old documents often looks right but is stored differently, so a search
/// for `น้ำ` does not find it:
///
/// - `ํ` + `า` (nikhahit and sara aa) for `ำ` (sara am), also with a tone
///   mark between or after them (`นํ้า` → `น้ำ`);
/// - a tone mark typed before the vowel above or below (`ก่ี` → `กี่`);
/// - the same mark typed twice (`ทั้้ง` → `ทั้ง`);
/// - two `เ` for `แ`;
/// - the private-use glyphs of old Thai fonts (U+F700–U+F71A) for the
///   letters and marks they draw.
pub fn normalize(input: &str) -> String {
    let chars: Vec<char> = input.chars().map(from_private_use).collect();
    let mut out: Vec<char> = Vec::with_capacity(chars.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        // ํ (+ tone) + า → (tone) + ำ
        if c == NIKHAHIT {
            let tones: Vec<char> = chars[i + 1..]
                .iter()
                .take_while(|&&t| is_tone(t))
                .copied()
                .collect();
            if chars.get(i + 1 + tones.len()) == Some(&SARA_AA) {
                out.extend(tones.iter().take(1));
                out.push(SARA_AM);
                i += 2 + tones.len();
                continue;
            }
        }
        // tone + ํ + า → tone + ำ (the tone is already out)
        if c == SARA_AA && out.last() == Some(&NIKHAHIT) {
            out.pop();
            out.push(SARA_AM);
            i += 1;
            continue;
        }
        // ำ before its tone mark: the tone goes first.
        if is_tone(c) && out.last() == Some(&SARA_AM) {
            out.pop();
            out.push(c);
            out.push(SARA_AM);
            i += 1;
            continue;
        }
        // A vowel above or below after the tone mark: it goes first.
        if is_upper_lower_vowel(c) {
            if let Some(&last) = out.last() {
                if is_tone(last) || last == THANTHAKHAT {
                    out.pop();
                    out.push(c);
                    out.push(last);
                    i += 1;
                    continue;
                }
            }
        }
        // The same combining mark twice.
        if is_mark(c) && out.last() == Some(&c) {
            i += 1;
            continue;
        }
        // เ + เ → แ
        if c == SARA_E && chars.get(i + 1) == Some(&SARA_E) {
            out.push(SARA_AE);
            i += 2;
            continue;
        }
        out.push(c);
        i += 1;
    }
    out.into_iter().collect()
}

const NIKHAHIT: char = '\u{0E4D}';
const SARA_AA: char = '\u{0E32}';
const SARA_AM: char = '\u{0E33}';
const SARA_E: char = '\u{0E40}';
const SARA_AE: char = '\u{0E41}';
const THANTHAKHAT: char = '\u{0E4C}';

fn is_tone(c: char) -> bool {
    ('\u{0E48}'..='\u{0E4B}').contains(&c)
}

/// ั ิ ี ึ ื ุ ู ฺ ็
fn is_upper_lower_vowel(c: char) -> bool {
    c == '\u{0E31}' || ('\u{0E34}'..='\u{0E3A}').contains(&c) || c == '\u{0E47}'
}

/// Any Thai combining mark.
fn is_mark(c: char) -> bool {
    is_upper_lower_vowel(c) || ('\u{0E48}'..='\u{0E4E}').contains(&c)
}

/// The Windows Thai fonts' private-use glyphs (shifted or tail-less forms
/// drawn by old PDF writers) as the characters they stand for.
fn from_private_use(c: char) -> char {
    const MAP: [char; 27] = [
        '\u{0E10}', // F700 ฐ without its tail
        '\u{0E34}', '\u{0E35}', '\u{0E36}',
        '\u{0E37}', // F701–F704 ิ ี ึ ื shifted left
        '\u{0E48}', '\u{0E49}', '\u{0E4A}', '\u{0E4B}',
        '\u{0E4C}', // F705–F709 shifted left
        '\u{0E48}', '\u{0E49}', '\u{0E4A}', '\u{0E4B}', '\u{0E4C}', // F70A–F70E lowered
        '\u{0E0D}', // F70F ญ without its tail
        '\u{0E31}', '\u{0E4D}', '\u{0E47}', // F710–F712 ั ํ ็ shifted left
        '\u{0E48}', '\u{0E49}', '\u{0E4A}', '\u{0E4B}',
        '\u{0E4C}', // F713–F717 lowered left
        '\u{0E38}', '\u{0E39}', '\u{0E3A}', // F718–F71A ุ ู ฺ lowered
    ];
    match c as u32 {
        n @ 0xF700..=0xF71A => MAP[(n - 0xF700) as usize],
        _ => c,
    }
}

/// Years swapped between the Buddhist Era and the Common Era: a stand-alone
/// four-digit number from 2400 to 2700 is read as พ.ศ. and loses 543, one
/// from 1800 to 2299 is read as ค.ศ. and gains 543. A `พ.ศ.`/`ค.ศ.` (or
/// `BE`/`CE`/`AD`) just before it is swapped along. Thai digits stay Thai.
pub fn swap_era(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < chars.len() {
        let starts = digit(chars[i]).is_some() && (i == 0 || !is_word_char(chars[i - 1]));
        if starts {
            let run: Vec<char> = chars[i..]
                .iter()
                .take_while(|&&c| digit(c).is_some())
                .copied()
                .collect();
            let ends = chars.get(i + run.len()).map_or(true, |&c| !is_word_char(c));
            if run.len() == 4 && ends {
                let year = run.iter().fold(0u32, |n, &c| n * 10 + digit(c).unwrap());
                let swapped = match year {
                    2400..=2700 => Some((year - 543, true)),
                    1800..=2299 => Some((year + 543, false)),
                    _ => None,
                };
                if let Some((swapped, to_ce)) = swapped {
                    swap_era_label(&mut out, to_ce);
                    let thai = run.iter().any(|&c| c > '9');
                    for d in swapped.to_string().chars() {
                        out.push(if thai {
                            char::from_u32(0x0E50 + d as u32 - '0' as u32).unwrap_or(d)
                        } else {
                            d
                        });
                    }
                    i += 4;
                    continue;
                }
            }
            out.extend(&run);
            i += run.len();
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// Swap the era label at the end of `out` (with any spaces after it).
fn swap_era_label(out: &mut String, to_ce: bool) {
    let trimmed = out.trim_end();
    let gap = out[trimmed.len()..].to_string();
    let pairs: [(&str, &str); 3] = [("พ.ศ.", "ค.ศ."), ("BE", "CE"), ("B.E.", "A.D.")];
    for (be, ce) in pairs {
        let (from, to) = if to_ce { (be, ce) } else { (ce, be) };
        if trimmed.ends_with(from) {
            let keep = trimmed.len() - from.len();
            out.truncate(keep);
            out.push_str(to);
            out.push_str(&gap);
            return;
        }
    }
    if !to_ce && trimmed.ends_with("AD") {
        let keep = trimmed.len() - 2;
        out.truncate(keep);
        out.push_str("BE");
        out.push_str(&gap);
    }
}

fn digit(c: char) -> Option<u32> {
    match c {
        '0'..='9' => Some(c as u32 - '0' as u32),
        '\u{0E50}'..='\u{0E59}' => Some(c as u32 - 0x0E50),
        _ => None,
    }
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '.' || c == ',' || c == '-' || c == '/' || c == ':'
}

const DIGITS: [&str; 10] = [
    "ศูนย์",
    "หนึ่ง",
    "สอง",
    "สาม",
    "สี่",
    "ห้า",
    "หก",
    "เจ็ด",
    "แปด",
    "เก้า",
];
const PLACES: [&str; 6] = ["", "สิบ", "ร้อย", "พัน", "หมื่น", "แสน"];

/// A whole number in Thai words (`1250` → `หนึ่งพันสองร้อยห้าสิบ`). A final
/// 1 after higher digits is `เอ็ด` (`101` → `หนึ่งร้อยเอ็ด`), as on cheques.
fn int_words(n: u64) -> String {
    if n == 0 {
        return DIGITS[0].to_string();
    }
    let mut out = String::new();
    let millions = n / 1_000_000;
    let rest = n % 1_000_000;
    if millions > 0 {
        out.push_str(&int_words(millions));
        out.push_str("ล้าน");
    }
    let digits: Vec<u32> = format!("{rest:06}")
        .chars()
        .map(|c| c as u32 - '0' as u32)
        .collect();
    for (k, &d) in digits.iter().enumerate() {
        let place = 5 - k;
        if d == 0 {
            continue;
        }
        match (place, d) {
            (1, 1) => out.push_str("สิบ"),
            (1, 2) => out.push_str("ยี่สิบ"),
            (0, 1) if n > 1 => out.push_str("เอ็ด"),
            _ => {
                out.push_str(DIGITS[d as usize]);
                out.push_str(PLACES[place]);
            }
        }
    }
    out
}

/// A number read from `input`: digits (Arabic or Thai), thousands commas,
/// one decimal point, a leading minus, and spaces or `บาท` around it.
/// `None` if there is anything else, or it is too large to read.
fn parse_number(input: &str) -> Option<(bool, u64, String)> {
    let s = input.trim();
    let s = s.strip_suffix("บาท").unwrap_or(s).trim();
    let (negative, s) = match s.strip_prefix('-') {
        Some(rest) => (true, rest.trim_start()),
        None => (false, s),
    };
    let (whole, frac) = s.split_once('.').unwrap_or((s, ""));
    if whole.is_empty() && frac.is_empty() {
        return None;
    }
    let mut int: u64 = 0;
    let mut seen = 0;
    for c in whole.chars() {
        if c == ',' {
            continue;
        }
        let d = digit(c)?;
        int = int.checked_mul(10)?.checked_add(d as u64)?;
        seen += 1;
    }
    if seen > 15 {
        return None;
    }
    let frac: String = frac
        .chars()
        .map(|c| digit(c).and_then(|d| char::from_digit(d, 10)))
        .collect::<Option<String>>()?;
    Some((negative, int, frac))
}

/// A number written out in Thai words (`1,250.5` → `หนึ่งพันสองร้อยห้าสิบจุดห้า`).
/// The selection is returned as it is when it is not a number.
pub fn number_words(input: &str) -> String {
    let Some((negative, int, frac)) = parse_number(input) else {
        return input.to_string();
    };
    let mut out = String::new();
    if negative {
        out.push_str("ลบ");
    }
    out.push_str(&int_words(int));
    if !frac.is_empty() {
        out.push_str("จุด");
        for c in frac.chars() {
            out.push_str(DIGITS[c as usize - '0' as usize]);
        }
    }
    out
}

/// An amount written out as on a cheque (`1,250.50` →
/// `หนึ่งพันสองร้อยห้าสิบบาทห้าสิบสตางค์`, `100` → `หนึ่งร้อยบาทถ้วน`).
/// Satang are rounded to two places. The selection is returned as it is
/// when it is not a number.
pub fn baht_words(input: &str) -> String {
    let Some((negative, mut int, frac)) = parse_number(input) else {
        return input.to_string();
    };
    let mut satang: u64 = frac
        .chars()
        .chain("000".chars())
        .take(3)
        .fold(0, |n, c| n * 10 + (c as u64 - '0' as u64));
    satang = (satang + 5) / 10;
    if satang == 100 {
        int += 1;
        satang = 0;
    }
    let mut out = String::new();
    if negative {
        out.push_str("ลบ");
    }
    if int > 0 || satang == 0 {
        out.push_str(&int_words(int));
        out.push_str("บาท");
    }
    if satang == 0 {
        out.push_str("ถ้วน");
    } else {
        out.push_str(&int_words(satang));
        out.push_str("สตางค์");
    }
    out
}

/// Spacing around Thai marks and brackets, as the Royal Institute's rules
/// for spacing put it:
///
/// - mai yamok `ๆ` stands apart from its word, with a space after it
///   (`เด็กๆเล่น` → `เด็ก ๆ เล่น`);
/// - `ฯลฯ` stands apart on both sides;
/// - paiyan noi `ฯ` (an abbreviation) joins its word (`กรุงเทพ ฯ` →
///   `กรุงเทพฯ`);
/// - brackets stand apart outside and hug their text inside
///   (`คำ(อธิบาย)ต่อ` → `คำ (อธิบาย) ต่อ`);
/// - spaces run together become one (not at the start of a line).
///
/// Only spaces are added or taken away; no character is changed.
pub fn tidy_spacing(input: &str) -> String {
    let thai = |c: char| ('\u{0E01}'..='\u{0E5B}').contains(&c);
    let chars: Vec<char> = input.chars().collect();
    let mut out: Vec<char> = Vec::with_capacity(chars.len() + 8);
    let space_before = |out: &mut Vec<char>| {
        if out
            .last()
            .is_some_and(|&c| c != ' ' && c != '\n' && c != '\t')
        {
            out.push(' ');
        }
    };
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        // ฯลฯ: apart on both sides.
        if c == 'ฯ' && next == Some('ล') && chars.get(i + 2) == Some(&'ฯ') {
            space_before(&mut out);
            out.extend(['ฯ', 'ล', 'ฯ']);
            i += 3;
            if chars.get(i).is_some_and(|&n| thai(n) || n == '(') {
                out.push(' ');
            }
            continue;
        }
        match c {
            'ๆ' => {
                space_before(&mut out);
                out.push('ๆ');
                if next.is_some_and(|n| thai(n) || n == '(') {
                    out.push(' ');
                }
            }
            'ฯ' => {
                // Joins the word before it.
                while out.last() == Some(&' ') {
                    out.pop();
                }
                out.push('ฯ');
            }
            '(' => {
                if out.last().is_some_and(|&p| thai(p)) {
                    out.push(' ');
                }
                out.push('(');
                while chars.get(i + 1) == Some(&' ') {
                    i += 1;
                }
            }
            ')' => {
                while out.last() == Some(&' ') {
                    out.pop();
                }
                out.push(')');
                if next.is_some_and(thai) {
                    out.push(' ');
                }
            }
            ' ' => {
                // Indentation: only spaces so far on this line.
                let line_start = out
                    .iter()
                    .rev()
                    .take_while(|&&p| p != '\n')
                    .all(|&p| p == ' ');
                if line_start || out.last() != Some(&' ') {
                    out.push(' ');
                }
            }
            _ => out.push(c),
        }
        i += 1;
    }
    out.into_iter().collect()
}

/// Characters the palette can type, found by name in either language:
/// (character, Thai name, English name).
pub const SYMBOLS: &[(&str, &str, &str)] = &[
    ("฿", "บาท", "baht"),
    ("ๆ", "ไม้ยมก", "mai yamok repeat"),
    ("ฯ", "ไปยาลน้อย", "paiyannoi abbreviation"),
    ("ฯลฯ", "ไปยาลใหญ่", "paiyanyai etc"),
    ("๏", "ฟองมัน", "fongman"),
    ("๚ะ", "อังคั่นวิสรรชนีย์", "end of chapter"),
    ("๛", "โคมูตร", "khomut end"),
    ("°", "องศา", "degree"),
    ("°C", "องศาเซลเซียส", "celsius"),
    ("×", "คูณ", "times multiply"),
    ("÷", "หาร", "divide"),
    ("±", "บวกลบ", "plus minus"),
    ("≈", "ประมาณ", "approximately"),
    ("≠", "ไม่เท่ากับ", "not equal"),
    ("≤", "น้อยกว่าหรือเท่ากับ", "less or equal"),
    ("≥", "มากกว่าหรือเท่ากับ", "greater or equal"),
    ("→", "ลูกศรขวา", "arrow right"),
    ("←", "ลูกศรซ้าย", "arrow left"),
    ("•", "จุดหัวข้อ", "bullet"),
    ("…", "จุดไข่ปลา", "ellipsis"),
    ("—", "ขีดยาว", "em dash"),
    ("–", "ขีดสั้น", "en dash range"),
    ("“”", "อัญประกาศ", "quotes double"),
    ("‘’", "อัญประกาศเดี่ยว", "quotes single"),
    ("©", "ลิขสิทธิ์", "copyright"),
    ("®", "เครื่องหมายการค้าจดทะเบียน", "registered"),
    ("™", "เครื่องหมายการค้า", "trademark"),
    ("€", "ยูโร", "euro"),
    ("£", "ปอนด์", "pound"),
    ("¥", "เยน", "yen"),
    ("½", "ครึ่ง", "half"),
    ("²", "ยกกำลังสอง", "squared"),
    ("³", "ยกกำลังสาม", "cubed"),
    ("✓", "เครื่องหมายถูก", "check tick"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thai_text_takes_its_standard_form() {
        assert_eq!(normalize("น\u{0E4D}\u{0E49}\u{0E32}"), "น้ำ");
        assert_eq!(normalize("น\u{0E49}\u{0E4D}\u{0E32}"), "น้ำ");
        assert_eq!(normalize("ท\u{0E4D}\u{0E32}"), "ทำ");
        assert_eq!(normalize("น\u{0E33}\u{0E49}"), "น้ำ");
        assert_eq!(normalize("ก\u{0E48}\u{0E35}"), "กี่");
        assert_eq!(normalize("ทั้\u{0E49}ง"), "ทั้ง");
        assert_eq!(normalize("เเมว"), "แมว");
        assert_eq!(normalize("ป\u{F701}ด"), "ปิด");
        assert_eq!(normalize("ผู\u{F70B}"), "ผู้");
        // Already standard: untouched, English and digits too.
        for s in ["น้ำ กี่ ทั้ง แมว", "เด็ก ๆ", "hello 123", "ไม้ยมก ๆ", "เเ"]
        {
            let expect = if s == "เเ" { "แ" } else { s };
            assert_eq!(normalize(s), expect, "{s}");
        }
    }

    #[test]
    fn years_swap_between_eras() {
        assert_eq!(swap_era("พ.ศ. 2569"), "ค.ศ. 2026");
        assert_eq!(swap_era("ค.ศ. 2026"), "พ.ศ. 2569");
        assert_eq!(swap_era("ปี 2569 และ 2570"), "ปี 2026 และ 2027");
        assert_eq!(swap_era("๒๕๖๙"), "๒๐๒๖");
        assert_eq!(swap_era("AD 2026"), "BE 2569");
        // Not years: kept.
        for s in [
            "12345",
            "02/10/2026",
            "1,250",
            "3000",
            "v2026",
            "เวลา 2569.5",
        ] {
            assert_eq!(swap_era(s), s, "{s}");
        }
    }

    #[test]
    fn numbers_are_written_out() {
        assert_eq!(number_words("0"), "ศูนย์");
        assert_eq!(number_words("11"), "สิบเอ็ด");
        assert_eq!(number_words("21"), "ยี่สิบเอ็ด");
        assert_eq!(number_words("101"), "หนึ่งร้อยเอ็ด");
        assert_eq!(number_words("1,250"), "หนึ่งพันสองร้อยห้าสิบ");
        assert_eq!(number_words("1250.5"), "หนึ่งพันสองร้อยห้าสิบจุดห้า");
        assert_eq!(number_words("๑๐"), "สิบ");
        assert_eq!(number_words("1000000"), "หนึ่งล้าน");
        assert_eq!(number_words("21000001"), "ยี่สิบเอ็ดล้านเอ็ด");
        assert_eq!(number_words("-5"), "ลบห้า");
        assert_eq!(number_words("hello"), "hello");
        assert_eq!(number_words("12a"), "12a");
    }

    #[test]
    fn amounts_are_written_as_on_a_cheque() {
        assert_eq!(baht_words("100"), "หนึ่งร้อยบาทถ้วน");
        assert_eq!(baht_words("1,250.50"), "หนึ่งพันสองร้อยห้าสิบบาทห้าสิบสตางค์");
        assert_eq!(baht_words("0.25"), "ยี่สิบห้าสตางค์");
        assert_eq!(baht_words("1.01"), "หนึ่งบาทหนึ่งสตางค์");
        assert_eq!(baht_words("21.21 บาท"), "ยี่สิบเอ็ดบาทยี่สิบเอ็ดสตางค์");
        assert_eq!(baht_words("9.999"), "สิบบาทถ้วน");
        assert_eq!(baht_words("0"), "ศูนย์บาทถ้วน");
        assert_eq!(baht_words("abc"), "abc");
    }

    #[test]
    fn thai_spacing_follows_the_royal_institute() {
        assert_eq!(tidy_spacing("เด็กๆเล่น"), "เด็ก ๆ เล่น");
        assert_eq!(tidy_spacing("เด็ก ๆ เล่น"), "เด็ก ๆ เล่น");
        assert_eq!(tidy_spacing("ผลไม้ฯลฯ"), "ผลไม้ ฯลฯ");
        assert_eq!(tidy_spacing("ผลไม้ฯลฯและ"), "ผลไม้ ฯลฯ และ");
        assert_eq!(tidy_spacing("กรุงเทพ ฯ"), "กรุงเทพฯ");
        assert_eq!(tidy_spacing("คำ(อธิบาย)ต่อ"), "คำ (อธิบาย) ต่อ");
        assert_eq!(tidy_spacing("คำ ( อธิบาย ) ต่อ"), "คำ (อธิบาย) ต่อ");
        assert_eq!(tidy_spacing("หนึ่ง   สอง"), "หนึ่ง สอง");
        // English, indentation and line ends are left alone.
        assert_eq!(tidy_spacing("f(x) = y"), "f(x) = y");
        assert_eq!(tidy_spacing("  ย่อหน้า\n  ต่อ"), "  ย่อหน้า\n  ต่อ");
        assert_eq!(tidy_spacing("จบ ๆ"), "จบ ๆ");
        // Only spaces move.
        for s in ["เด็กๆ(ฯลฯ)กรุงเทพ ฯ  x", "a ( b ) c", "ๆๆ ฯฯ"]
        {
            let keep = |t: &str| t.chars().filter(|c| *c != ' ').collect::<String>();
            assert_eq!(keep(&tidy_spacing(s)), keep(s), "{s}");
        }
    }
}
