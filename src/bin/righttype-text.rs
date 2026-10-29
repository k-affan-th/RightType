//! Bounded stdin/stdout bridge to the shared repair engine. Text never travels
//! through command-line arguments, temporary files, or diagnostic messages.

use std::io::{self, Read, Write};

use righttype::{dict, repair};
use zeroize::{Zeroize, Zeroizing};

// shortcut: limit preview passages to 32 KiB; revisit for document-sized repair.
const MAX_BYTES: usize = 32 * 1024;

fn run(input: impl Read, mut output: impl Write) -> Result<(), &'static str> {
    let mut bytes = Zeroizing::new(Vec::new());
    input
        .take((MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "Could not read text.")?;
    if bytes.len() > MAX_BYTES {
        return Err("Text exceeds the 32 KiB preview limit.");
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| "Text must be UTF-8.")?;
    let mut repaired = repair::repair(text, dict::english(), dict::thai());
    let result = output
        .write_all(repaired.text.as_bytes())
        .and_then(|()| output.flush())
        .map_err(|_| "Could not return the result.");
    repaired.text.zeroize();
    for change in &mut repaired.changes {
        change.original.zeroize();
        change.fixed.zeroize();
    }
    result
}

fn main() {
    let result = if std::env::args_os().len() != 1 {
        Err("Pass UTF-8 text on stdin, without arguments.")
    } else {
        run(io::stdin().lock(), io::stdout().lock())
    };
    if let Err(message) = result {
        eprintln!("{message}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed(input: &[u8]) -> Result<String, &'static str> {
        let mut output = Vec::new();
        run(input, &mut output)?;
        Ok(String::from_utf8(output).unwrap())
    }

    #[test]
    fn uses_shared_policy_in_both_directions_and_keeps_spacing() {
        assert_eq!(
            fixed("l;ylfu\tแนพพำแะ\nhello สวัสดี 😀  ".as_bytes()).unwrap(),
            "สวัสดี\tcorrect\nhello สวัสดี 😀  "
        );
        assert_eq!(fixed(b"").unwrap(), "");
    }

    #[test]
    fn shared_secret_guard_leaves_keys_untouched() {
        let text = b"e9873d79c6d87dc0fb6a5778633389f4453213303da61f20bd67fc233aa33262";
        assert_eq!(fixed(text).unwrap().as_bytes(), text);
    }

    #[test]
    fn rejects_invalid_or_oversized_input_without_partial_output() {
        for bytes in [vec![0xff], vec![b'a'; MAX_BYTES + 1]] {
            let mut output = Vec::new();
            assert!(run(bytes.as_slice(), &mut output).is_err());
            assert!(output.is_empty());
        }
        assert!(fixed(&vec![b' '; MAX_BYTES]).is_ok());
    }

    #[test]
    fn reports_io_failure_without_echoing_text() {
        struct Broken;
        impl Write for Broken {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        assert_eq!(
            run(&b"l;ylfu"[..], Broken),
            Err("Could not return the result.")
        );
    }
}
