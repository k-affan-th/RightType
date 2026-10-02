//! What other programs already do to typed text, so RightType neither
//! repeats it nor mistakes it for a fault.
//!
//! - Word, Outlook, Google Docs and others rewrite a word after it is typed
//!   (a capital at the start of a sentence, curly quotes, `--` to `—`, their
//!   own AutoCorrect list). The check-after-write reads that back as "not
//!   what was sent"; [`rewritten_by_app`] tells such a rewrite from garbling.
//! - Browsers, Electron apps, Word and LibreOffice already move and delete by
//!   Thai word with Ctrl ([`breaks_thai_words`]): RightType leaves
//!   Ctrl+Backspace to them.
//! - Google Docs draws its text itself and shares none of it unless its
//!   screen-reader support is on ([`is_google_docs`]).
//!
//! Plain functions of names and text: no OS calls.

/// Programs that delete one Thai word on Ctrl+Backspace themselves
/// (Chromium and Firefox use ICU's Thai dictionary; Office and LibreOffice
/// their own word breaking).
const THAI_WORD_BREAKING: &[&str] = &[
    // Browsers.
    "chrome.exe",
    "msedge.exe",
    "brave.exe",
    "opera.exe",
    "vivaldi.exe",
    "firefox.exe",
    // Electron apps.
    "code.exe",
    "cursor.exe",
    "slack.exe",
    "discord.exe",
    "ms-teams.exe",
    "teams.exe",
    "notion.exe",
    "obsidian.exe",
    "claude.exe",
    "whatsapp.exe",
    "line.exe",
    // Office and LibreOffice.
    "winword.exe",
    "outlook.exe",
    "powerpnt.exe",
    "onenote.exe",
    "soffice.bin",
    "soffice.exe",
];

/// Does `exe` already delete by Thai word with Ctrl+Backspace?
pub fn breaks_thai_words(exe: &str) -> bool {
    THAI_WORD_BREAKING.contains(&exe.to_ascii_lowercase().as_str())
}

/// Programs whose fields fill in the rest of what is typed and select it,
/// like a browser's address bar (Excel's AutoComplete in a cell).
pub fn completes_inline(exe: &str) -> bool {
    exe.eq_ignore_ascii_case("excel.exe")
}

/// Is a browser window with this title Google Docs (or Sheets, Slides)?
pub fn is_google_docs(title: &str) -> bool {
    [" - Google Docs", " - Google Sheets", " - Google Slides"]
        .iter()
        .any(|t| title.contains(t))
}

/// One character as an app's AutoFormat leaves it: curly quotes and long
/// dashes back to the plain ones, and letters in lower case.
fn plain(c: char) -> char {
    match c {
        '\u{201C}' | '\u{201D}' | '\u{201E}' => '"',
        '\u{2018}' | '\u{2019}' | '\u{201A}' => '\'',
        '\u{2013}' | '\u{2014}' => '-',
        '\u{00A0}' => ' ',
        c => c.to_lowercase().next().unwrap_or(c),
    }
}

/// `sent` was typed and the app shows `shown` (its last characters) instead:
/// is that the app's own rewriting rather than text lost or garbled?
///
/// - the same text but for capitals, curly quotes and long dashes; or
/// - the word was replaced by another real word (`is_word`) — an
///   AutoCorrect list — of about the same length.
///
/// Garbling leaves repeated marks or broken words (`ีีีีีี` for สวัสดี,
/// 5 of 6 characters), which is neither.
pub fn rewritten_by_app(sent: &str, shown: &str, is_word: impl Fn(&str) -> bool) -> bool {
    let sent_plain: Vec<char> = sent.chars().map(plain).collect();
    let shown_plain: Vec<char> = shown.chars().map(plain).collect();
    if shown_plain.ends_with(&sent_plain) {
        return true;
    }
    // `--` becomes one dash.
    let squeeze = |v: &[char]| -> Vec<char> {
        let mut out: Vec<char> = Vec::with_capacity(v.len());
        for &c in v {
            if c == '-' && out.last() == Some(&'-') {
                continue;
            }
            out.push(c);
        }
        out
    };
    if squeeze(&shown_plain).ends_with(&squeeze(&sent_plain)) {
        return true;
    }
    // A whole word replaced: the last word shown is a real word, close in
    // length to the last word sent, and not what was sent.
    let last_word = |s: &str| -> String {
        s.trim_end()
            .rsplit(char::is_whitespace)
            .next()
            .unwrap_or("")
            .to_string()
    };
    let (a, b) = (last_word(sent), last_word(shown));
    let (la, lb) = (a.chars().count(), b.chars().count());
    !b.is_empty() && a != b && la.abs_diff(lb) <= 2 && is_word(&b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(w: &str) -> bool {
        ["the", "Hello", "hello", "สวัสดี"].contains(&w)
    }

    #[test]
    fn an_apps_own_rewriting_is_not_garbling() {
        // Capital at the start of a sentence.
        assert!(rewritten_by_app("correct ", "Correct ", word));
        // Curly quotes and a long dash.
        assert!(rewritten_by_app(
            "\"hi\" -- ok",
            "\u{201C}hi\u{201D} \u{2014} ok",
            word
        ));
        // AutoCorrect: teh → the.
        assert!(rewritten_by_app("teh", "the", word));
        // Garbled Thai, or a word cut short, is still garbling.
        assert!(!rewritten_by_app("สวัสดี", "ีีีีีี", word));
        assert!(!rewritten_by_app("สวัสดี", "สวัสด", word));
        assert!(!rewritten_by_app("hello", "helo", word));
    }

    #[test]
    fn apps_with_their_own_thai_word_breaking() {
        assert!(breaks_thai_words("chrome.exe"));
        assert!(breaks_thai_words("WINWORD.EXE"));
        assert!(!breaks_thai_words("notepad.exe"));
        assert!(completes_inline("EXCEL.EXE"));
        assert!(!completes_inline("notepad.exe"));
        assert!(is_google_docs("Report - Google Docs - Google Chrome"));
        assert!(!is_google_docs("Google - Google Chrome"));
    }
}
