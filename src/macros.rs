//! Macros (2.4): a snippet whose text also presses keys, waits, runs one of
//! the app's commands or sets a Word style — `{กด Ctrl+B}ตัวหนา{กด Ctrl+B}`,
//! `{สไตล์ Heading 1}รายงาน {วันที่}{กด Enter}`. Written in the snippet editor
//! like any snippet, run by its trigger or from the list at the cursor.
//!
//! Only what the typist wrote is run; nothing is recorded from typing. The
//! steps are read here; pressing them is the Windows side's. No OS calls.

/// One thing a macro does, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Typed as it is (date and time fields filled when it runs).
    Text(String),
    /// A key or a shortcut: `Enter`, `Ctrl+Alt+1`.
    Keys(String),
    /// Milliseconds, for the app to catch up (a dialog opening).
    Wait(u32),
    /// One of the app's own commands, by name (as the list at the cursor
    /// shows it): `Heading 1`, `หัวเรื่อง 1`.
    Command(String),
    /// A Word style by name: Ctrl+Shift+S, the name, Enter.
    Style(String),
    /// The copied text, typed (read only when the macro runs).
    Clipboard,
}

/// Why a macro cannot be saved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    /// `{กด …}` names a key RightType does not know.
    Keys(String),
    /// `{รอ …}` is not a number of milliseconds up to [`MAX_WAIT`].
    Wait(String),
    /// More than [`MAX_STEPS`] steps.
    TooLong,
    /// `{คำสั่ง}` or `{สไตล์}` without a name.
    NoName,
}

/// The longest single wait (ms), and all waits together.
pub const MAX_WAIT: u32 = 5000;
pub const MAX_WAIT_TOTAL: u32 = 15_000;
/// The most steps one macro has.
pub const MAX_STEPS: usize = 60;

/// The step names, in English and Thai.
const NAMES: &[(&str, Kind)] = &[
    ("keys", Kind::Keys),
    ("key", Kind::Keys),
    ("press", Kind::Keys),
    ("กด", Kind::Keys),
    ("wait", Kind::Wait),
    ("รอ", Kind::Wait),
    ("command", Kind::Command),
    ("cmd", Kind::Command),
    ("คำสั่ง", Kind::Command),
    ("style", Kind::Style),
    ("สไตล์", Kind::Style),
    ("ลักษณะ", Kind::Style),
    ("clipboard", Kind::Clipboard),
    ("คลิปบอร์ด", Kind::Clipboard),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Keys,
    Wait,
    Command,
    Style,
    Clipboard,
}

/// `{name arg}` at the start of `s`: its kind, argument and length.
fn field(s: &str) -> Option<(Kind, &str, usize)> {
    let inner = s.strip_prefix('{')?;
    let close = inner.find('}')?;
    let body = &inner[..close];
    let (name, arg) = match body.find(' ') {
        Some(i) => (&body[..i], body[i + 1..].trim()),
        None => (body, ""),
    };
    let kind = NAMES
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, k)| *k)?;
    Some((kind, arg, close + 2))
}

/// Key names as the shortcut table writes them: `ctrl+b` → `Ctrl+B`,
/// `enter` → `Enter`, Thai names too (`ปุ่มลบ` is not one: kept short).
pub fn tidy_keys(keys: &str) -> String {
    keys.split('+')
        .map(|p| {
            let p = p.trim();
            let lower = p.to_lowercase();
            match lower.as_str() {
                "ctrl" | "control" | "คอนโทรล" => "Ctrl".to_string(),
                "shift" | "ชิฟต์" => "Shift".to_string(),
                "alt" | "อัลต์" => "Alt".to_string(),
                "win" | "windows" => "Win".to_string(),
                "enter" | "return" | "เอนเทอร์" | "ขึ้นบรรทัด" => {
                    "Enter".to_string()
                }
                "esc" | "escape" => "Esc".to_string(),
                "tab" | "แท็บ" => "Tab".to_string(),
                "space" | "เว้นวรรค" => "Space".to_string(),
                "del" | "delete" => "Delete".to_string(),
                "backspace" | "bksp" => "Backspace".to_string(),
                "pageup" | "pgup" => "PageUp".to_string(),
                "pagedown" | "pgdn" => "PageDown".to_string(),
                _ if p.chars().count() == 1 => p.to_uppercase(),
                _ => {
                    let mut c = lower.chars();
                    c.next()
                        .map(|f| f.to_uppercase().chain(c).collect())
                        .unwrap_or_default()
                }
            }
        })
        .collect::<Vec<_>>()
        .join("+")
}

/// Whether `keys` can be pressed (a name the shortcut table knows).
pub fn keys_known(keys: &str) -> bool {
    crate::shortcuts::virtual_keys(keys).is_some()
}

/// The steps of a snippet's text, or `None` for a plain snippet (no step
/// fields: typed as before). Line breaks become Enter.
pub fn steps(text: &str) -> Option<Result<Vec<Step>, Problem>> {
    let mut steps = Vec::new();
    let mut plain = String::new();
    let mut any = false;
    let mut rest = text;
    let flush = |plain: &mut String, steps: &mut Vec<Step>| {
        for (i, line) in plain.split('\n').enumerate() {
            if i > 0 {
                steps.push(Step::Keys("Enter".into()));
            }
            if !line.is_empty() {
                steps.push(Step::Text(line.to_string()));
            }
        }
        plain.clear();
    };
    while !rest.is_empty() {
        if let Some((kind, arg, len)) = field(rest) {
            any = true;
            flush(&mut plain, &mut steps);
            let step = match kind {
                Kind::Keys => {
                    let keys = tidy_keys(arg);
                    if !keys_known(&keys) {
                        return Some(Err(Problem::Keys(arg.to_string())));
                    }
                    Step::Keys(keys)
                }
                Kind::Wait => match arg.trim_end_matches("ms").trim().parse::<u32>() {
                    Ok(ms) if ms <= MAX_WAIT => Step::Wait(ms),
                    _ => return Some(Err(Problem::Wait(arg.to_string()))),
                },
                Kind::Command if arg.is_empty() => return Some(Err(Problem::NoName)),
                Kind::Command => Step::Command(arg.to_string()),
                Kind::Style if arg.is_empty() => return Some(Err(Problem::NoName)),
                Kind::Style => Step::Style(arg.to_string()),
                Kind::Clipboard => Step::Clipboard,
            };
            steps.push(step);
            rest = &rest[len..];
        } else {
            let c = rest.chars().next()?;
            plain.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    if !any {
        return None;
    }
    flush(&mut plain, &mut steps);
    let waits: u32 = steps
        .iter()
        .map(|s| match s {
            Step::Wait(ms) => *ms,
            _ => 0,
        })
        .sum();
    if steps.len() > MAX_STEPS || waits > MAX_WAIT_TOTAL {
        return Some(Err(Problem::TooLong));
    }
    Some(Ok(steps))
}

/// Is this snippet's text a macro?
pub fn is_macro(text: &str) -> bool {
    steps(text).is_some()
}

/// A short line saying what a macro does, for the list at the cursor:
/// `Heading 1 · "Report 4 Oct" · Enter`.
pub fn summary(steps: &[Step], thai: bool) -> String {
    steps
        .iter()
        .map(|s| match s {
            Step::Text(t) => format!("“{t}”"),
            Step::Keys(k) => k.clone(),
            Step::Wait(ms) => format!("{ms} ms"),
            Step::Command(c) | Step::Style(c) => c.clone(),
            Step::Clipboard => if thai {
                "คลิปบอร์ด"
            } else {
                "clipboard"
            }
            .to_string(),
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_snippets_are_not_macros() {
        assert_eq!(steps("Best regards,\nSomchai"), None);
        assert_eq!(steps("วันที่ {วันที่}"), None);
        assert_eq!(steps("{not a step}"), None);
    }

    #[test]
    fn steps_in_order() {
        let s = steps("{style Heading 1}Report {date}{กด enter}{รอ 200}{คำสั่ง Bold}\nend")
            .unwrap()
            .unwrap();
        assert_eq!(
            s,
            vec![
                Step::Style("Heading 1".into()),
                Step::Text("Report {date}".into()),
                Step::Keys("Enter".into()),
                Step::Wait(200),
                Step::Command("Bold".into()),
                Step::Keys("Enter".into()),
                Step::Text("end".into()),
            ]
        );
        assert_eq!(
            steps("{กด ctrl+b}ตัวหนา{กด Ctrl+B}").unwrap().unwrap()[0],
            Step::Keys("Ctrl+B".into())
        );
        assert_eq!(
            steps("{คลิปบอร์ด}{กด Ctrl+Shift+F12}").unwrap().unwrap(),
            vec![Step::Clipboard, Step::Keys("Ctrl+Shift+F12".into())]
        );
        assert!(summary(&s, false).starts_with("Heading 1 · “Report {date}” · Enter"));
    }

    #[test]
    fn problems() {
        assert_eq!(
            steps("{กด Ctrl+Nope}"),
            Some(Err(Problem::Keys("Ctrl+Nope".into())))
        );
        assert_eq!(
            steps("{wait 9000}"),
            Some(Err(Problem::Wait("9000".into())))
        );
        assert_eq!(
            steps("{wait soon}"),
            Some(Err(Problem::Wait("soon".into())))
        );
        assert_eq!(steps("{คำสั่ง}"), Some(Err(Problem::NoName)));
        assert_eq!(steps(&"{wait 5000}".repeat(4)), Some(Err(Problem::TooLong)));
        assert!(keys_known("Backspace"));
        assert_eq!(
            crate::snippets::check(";x", "{กด Nope}", crate::snippets::Scope::Either),
            Err(crate::snippets::Problem::MacroStep)
        );
    }
}
