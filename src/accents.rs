//! Holding a key to pick a character it does not type (as on a phone): a
//! numbered list near the cursor; a digit (or Esc) answers. Plain data: no
//! OS calls.

/// The characters offered when the key that typed `typed` is held, in the
/// order of their numbers (1, 2, …). Capitals offer capitals.
pub fn choices(typed: char) -> Option<Vec<String>> {
    let lower = typed.to_lowercase().next()?;
    let base: &[&str] = match lower {
        '.' => &["…", "·", "•"],
        '-' => &["–", "—", "±"],
        '"' => &["“", "”", "„"],
        '\'' => &["‘", "’"],
        '$' => &["฿", "€", "£", "¥"],
        '*' => &["×"],
        '/' => &["÷"],
        'a' => &["á", "à", "â", "ä", "ã"],
        'e' => &["é", "è", "ê", "ë"],
        'i' => &["í", "ì", "î", "ï"],
        'o' => &["ó", "ò", "ô", "ö", "õ"],
        'u' => &["ú", "ù", "û", "ü"],
        'n' => &["ñ"],
        'c' => &["ç"],
        'ๆ' => &["ฯ", "ฯลฯ"],
        '0'..='9' => {
            let thai = char::from_u32('๐' as u32 + (lower as u32 - '0' as u32))?;
            return Some(vec![thai.to_string()]);
        }
        _ => return None,
    };
    let capital = typed != lower;
    Some(
        base.iter()
            .map(|s| {
                if capital {
                    s.to_uppercase()
                } else {
                    s.to_string()
                }
            })
            .collect(),
    )
}

/// The list as shown near the cursor: `1 …   2 ·   3 •`.
pub fn shown(choices: &[String]) -> String {
    choices
        .iter()
        .enumerate()
        .map(|(i, c)| format!("{} {c}", i + 1))
        .collect::<Vec<_>>()
        .join("   ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn held_keys_offer_their_characters() {
        assert_eq!(choices('.').unwrap(), ["…", "·", "•"]);
        assert_eq!(choices('e').unwrap()[0], "é");
        assert_eq!(choices('E').unwrap()[0], "É");
        assert_eq!(choices('7').unwrap(), ["๗"]);
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
        assert_eq!(shown(&choices('.').unwrap()), "1 …   2 ·   3 •");
    }
}
