//! Key sequences through the hook's Auto-mode replay, for comparing with the
//! real hook (accuracy plan P2).
//!
//!     cargo run --release --example parity_sim -- --gen > examples/data/parity_sequences.txt
//!     cargo run --release --example parity_sim            # the screen after each sequence
//!     python e2e/parity.py                                 # the same keys into Notepad
//!
//! A sequence is one line: keys named by the character they type on US QWERTY
//! (upper case and `: ? ! " ( ) @` mean Shift is held), and `{SP}` `{ENTER}`
//! `{TAB}` `{BS}` `{LEFT}` `{HOME}` `{ESC}` for the other keys. Every sequence
//! starts on the US layout with an empty text box.
//!
//! `--gen` writes the committed sequence file: Thai typed on the English layout
//! (keys from `th_to_en`), headings with a colon, English words, names and
//! typos, a caret move in the middle of a word, Backspace, numbers, addresses.

use righttype::dict;
use righttype::layout::th_to_en;
use righttype::policy::InputLayout;
use righttype::sim::Engine;

const FILE: &str = include_str!("data/parity_sequences.txt");

fn run(line: &str) -> String {
    let mut e = Engine::new(dict::english(), dict::thai(), InputLayout::UsQwerty);
    let mut rest = line;
    while let Some(c) = rest.chars().next() {
        if c == '{' {
            let end = rest.find('}').expect("closing brace");
            match &rest[1..end] {
                "SP" => e.boundary(' '),
                "ENTER" => e.boundary(char::from(10u8)),
                "TAB" => e.boundary(char::from(9u8)),
                "BS" => e.backspace(),
                "LEFT" | "HOME" | "ESC" => e.caret_moved_by_key(),
                other => panic!("unknown key {other}"),
            }
            rest = &rest[end + 1..];
        } else {
            e.key(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    e.screen
}

fn generate() {
    let thai = [
        "สวัสดี",
        "ขอบคุณ",
        "ภาษาไทย",
        "ทำไม",
        "ประเทศไทย",
        "คอมพิวเตอร์",
        "ไม่เป็นไร",
        "ขอโทษ",
        "วันนี้อากาศดี",
        "เมื่อวานนี้",
    ];
    let mut lines: Vec<String> = Vec::new();
    for w in thai {
        lines.push(format!("{}{{SP}}", th_to_en(w)));
    }
    lines.push(format!("{}{{ENTER}}", th_to_en("สวัสดี")));
    lines.push(format!("{}{{TAB}}", th_to_en("ขอบคุณ")));
    for w in ["คำสำคัญ", "ภาษาไทย", "เพื่อการประมวลผลภาษาไทย"]
    {
        lines.push(format!("{}:{{SP}}", th_to_en(w)));
    }
    lines.push(format!("{}?{{SP}}", th_to_en("ทำไม")));
    for w in [
        "hello",
        "world",
        "computer",
        "different",
        "github",
        "login",
        "middleware",
        "Hello",
    ] {
        lines.push(format!("{w}{{SP}}"));
    }
    for w in [
        "clau",
        "figm",
        "logi",
        "Makro",
        "Claude",
        "claude",
        "anthropic",
        "kubernetes",
    ] {
        lines.push(format!("{w}{{SP}}"));
        lines.push(format!("{w}{{ENTER}}"));
    }
    for w in ["itme", "wrok", "gril", "msking", "cmoputer", "recieve"] {
        lines.push(format!("{w}{{SP}}"));
    }
    for (w, key) in [
        ("xhec", "LEFT"),
        ("voew", "HOME"),
        ("vuildin", "ESC"),
        ("lboc", "LEFT"),
        ("hello", "LEFT"),
    ] {
        lines.push(format!("{w}{{{key}}}"));
    }
    lines.push(format!("hello{{SP}}{}{{SP}}", th_to_en("สวัสดี")));
    lines.push(format!("{}{{SP}}hello{{SP}}", th_to_en("ขอบคุณ")));
    lines.push(format!(
        "{}{{SP}}{}{{SP}}",
        th_to_en("วันนี้"),
        th_to_en("ฝนตก")
    ));
    lines.push(format!("{}{{BS}}{{BS}}{{SP}}", th_to_en("สวัสดี")));
    lines.push("helo{BS}lo{SP}".to_string());
    lines.push("worl{BS}{BS}ld{SP}".to_string());
    for n in ["123", "3.14", "2026", "10:30", "1,250", "100%"] {
        lines.push(format!("{n}{{SP}}"));
    }
    for a in ["name@gmail.com", "www.google.com", "example.com"] {
        lines.push(format!("{a}{{SP}}"));
    }
    lines.push("git{SP}status{ENTER}".to_string());
    lines.push("cd{SP}src{ENTER}".to_string());
    for l in lines {
        println!("{l}");
    }
}

fn main() {
    if std::env::args().any(|a| a == "--gen") {
        generate();
        return;
    }
    for (i, line) in FILE.lines().filter(|l| !l.is_empty()).enumerate() {
        let screen = run(line).replace('\n', "\\n");
        println!("{i}\t{line}\t{screen}");
    }
}
