//! Holding a key to pick a character it does not type (as on a phone): a
//! numbered list near the cursor; a digit (or Esc) answers. Plain data: no
//! OS calls.

/// The characters offered when the key that typed `typed` is held, in the
/// order of their numbers (1, 2, …). Capitals offer capitals.
pub fn choices(typed: char) -> Option<Vec<String>> {
    let lower = typed.to_lowercase().next()?;
    let base: &[&str] = match lower {
        // Digits: raised, lowered, fractions, Thai — ¹²³⁴ and x₂ as easily
        // as on a phone keyboard.
        '0' => &["⁰", "₀", "๐", "°"],
        '1' => &["¹", "₁", "½", "¼", "๑"],
        '2' => &["²", "₂", "⅔", "๒"],
        '3' => &["³", "₃", "¾", "⅓", "๓"],
        '4' => &["⁴", "₄", "๔"],
        '5' => &["⁵", "₅", "๕"],
        '6' => &["⁶", "₆", "๖"],
        '7' => &["⁷", "₇", "๗"],
        '8' => &["⁸", "₈", "๘", "∞"],
        '9' => &["⁹", "₉", "๙"],
        // Maths.
        '+' => &["±", "⁺", "₊"],
        '-' => &["–", "—", "−", "±", "⁻", "₋"],
        '=' => &["≠", "≈", "≡", "≤", "≥", "⁼"],
        '<' => &["≤", "←", "«", "‹"],
        '>' => &["≥", "→", "»", "›"],
        '(' => &["⁽", "₍"],
        ')' => &["⁾", "₎"],
        '*' => &["×", "•", "★"],
        '/' => &["÷", "⁄", "½"],
        '^' => &["°", "ˆ"],
        '~' => &["≈", "∼"],
        '%' => &["‰", "°"],
        '!' => &["¡", "≠"],
        '?' => &["¿"],
        '#' => &["№", "♯"],
        '.' => &["…", "·", "•", "°"],
        '"' => &["“", "”", "„", "«", "»"],
        '\'' => &["‘", "’", "‚"],
        '$' => &["฿", "€", "£", "¥", "¢"],
        // Letters.
        'a' => &["á", "à", "â", "ä", "ã", "å", "æ", "ā", "α"],
        'b' => &["β"],
        'c' => &["ç", "ć", "č", "©"],
        'd' => &["ð", "δ"],
        'e' => &["é", "è", "ê", "ë", "ē", "ę", "€"],
        'g' => &["ğ", "γ"],
        'i' => &["í", "ì", "î", "ï", "ī"],
        'l' => &["ł", "λ"],
        'm' => &["µ"],
        'n' => &["ñ", "ń", "ⁿ"],
        'o' => &["ó", "ò", "ô", "ö", "õ", "ø", "œ", "ō", "°"],
        'p' => &["π", "¶"],
        'r' => &["®", "ř"],
        's' => &["ß", "ś", "š", "§", "σ"],
        't' => &["™", "θ", "þ"],
        'u' => &["ú", "ù", "û", "ü", "ū"],
        'x' => &["×", "ˣ"],
        'y' => &["ý", "ÿ"],
        'z' => &["ž", "ź", "ż"],
        'ๆ' => &["ฯ", "ฯลฯ"],
        _ => return None,
    };
    let capital = typed != lower;
    Some(
        base.iter()
            .filter_map(|s| {
                if !capital {
                    return Some(s.to_string());
                }
                // A capital where there is one of the same length (not ß → SS).
                let up = s.to_uppercase();
                (up.chars().count() == s.chars().count()).then_some(up)
            })
            .collect(),
    )
}

/// The list as shown near the cursor: `1 …   2 ·   3 •`.
pub fn shown(choices: &[String]) -> String {
    choices
        .iter()
        .enumerate()
        .map(|(i, c)| format!("{}{c}", i + 1))
        .collect::<Vec<_>>()
        .join("  ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn held_keys_offer_their_characters() {
        assert_eq!(choices('.').unwrap(), ["…", "·", "•", "°"]);
        assert_eq!(choices('e').unwrap()[0], "é");
        assert_eq!(choices('E').unwrap()[0], "É");
        assert_eq!(choices('7').unwrap(), ["⁷", "₇", "๗"]);
        assert_eq!(choices('2').unwrap()[0], "²");
        assert_eq!(choices('=').unwrap()[0], "≠");
        assert!(choices('S').unwrap().iter().all(|c| c.chars().count() == 1));
        assert!(choices('x').unwrap().contains(&"×".to_string()));
        // Never more than the number keys can pick.
        for c in "0123456789+-=<>()*/^~%!?#.\"'$abcdegilmnoprstuxyzๆ".chars() {
            assert!(choices(c).unwrap().len() <= 9, "{c}");
        }
        assert_eq!(choices('$').unwrap()[0], "฿");
        assert_eq!(choices('ๆ').unwrap(), ["ฯ", "ฯลฯ"]);
        assert!(choices('k').is_none());
        assert!(choices('ก').is_none());
        // No list is longer than the digits that pick from it.
        for c in ".-\"'$*/aeioun c0123456789ๆ".chars() {
            if let Some(list) = choices(c) {
                assert!(list.len() <= 9, "{c}");
            }
        }
        assert_eq!(shown(&choices('.').unwrap()), "1…  2·  3•  4°");
    }
}
