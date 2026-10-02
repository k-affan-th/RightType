//! Why the last word was fixed, or left as typed — for the palette's
//! "Why?" row. Only the reason is kept, never the word.

use crate::dict::Dictionary;
use crate::i18n::T;
use crate::policy::InputLayout;

/// The reason for what happened to the last word typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Why {
    /// Nothing typed yet (or since the last change of field).
    Nothing = 0,
    FixedWord,
    FixedWords,
    FixedAddress,
    FixedNumber,
    FixedSpelling,
    FixedCaps,
    FixedCode,
    Suggested,
    KeptManual,
    KeptEnglish,
    KeptThai,
    KeptUnknown,
    KeptSeed,
}

impl Why {
    const ALL: [Why; 14] = [
        Why::Nothing,
        Why::FixedWord,
        Why::FixedWords,
        Why::FixedAddress,
        Why::FixedNumber,
        Why::FixedSpelling,
        Why::FixedCaps,
        Why::FixedCode,
        Why::Suggested,
        Why::KeptManual,
        Why::KeptEnglish,
        Why::KeptThai,
        Why::KeptUnknown,
        Why::KeptSeed,
    ];

    pub fn from_u8(v: u8) -> Why {
        Why::ALL.get(v as usize).copied().unwrap_or(Why::Nothing)
    }

    /// The sentence that says it.
    pub fn message(self) -> T {
        match self {
            Why::Nothing => T::WhyNothing,
            Why::FixedWord => T::WhyFixedWord,
            Why::FixedWords => T::WhyFixedWords,
            Why::FixedAddress => T::WhyFixedAddress,
            Why::FixedNumber => T::WhyFixedNumber,
            Why::FixedSpelling => T::WhyFixedSpelling,
            Why::FixedCaps => T::WhyFixedCaps,
            Why::FixedCode => T::WhyFixedCode,
            Why::Suggested => T::WhySuggested,
            Why::KeptManual => T::WhyKeptManual,
            Why::KeptEnglish => T::WhyKeptEnglish,
            Why::KeptThai => T::WhyKeptThai,
            Why::KeptUnknown => T::WhyKeptUnknown,
            Why::KeptSeed => T::WhyKeptSeed,
        }
    }
}

/// What a fix was, from the text it put in.
pub fn fixed(corrected: &str, whole_words: bool) -> Why {
    if crate::english::is_email(corrected) || crate::english::is_web_address(corrected) {
        Why::FixedAddress
    } else if corrected.chars().any(|c| c.is_ascii_digit())
        && corrected
            .chars()
            .all(|c| c.is_ascii_digit() || ",.:/-%$฿".contains(c))
    {
        Why::FixedNumber
    } else if whole_words {
        Why::FixedWord
    } else {
        Why::FixedWords
    }
}

/// Why `word`, typed with `layout`, was left as typed (no fix found).
pub fn kept(word: &str, layout: InputLayout, en: &Dictionary, th: &Dictionary) -> Why {
    match layout {
        InputLayout::UsQwerty => {
            if crate::english::is_word(word, en) || crate::english::is_compound(word) {
                Why::KeptEnglish
            } else {
                Why::KeptUnknown
            }
        }
        InputLayout::ThaiKedmanee => {
            if th.contains(word) || crate::segment::is_fully_known(word, th) {
                Why::KeptThai
            } else {
                Why::KeptUnknown
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reasons_are_told_apart() {
        let (en, th) = (crate::dict::english(), crate::dict::thai());
        assert_eq!(
            kept("hello", InputLayout::UsQwerty, en, th),
            Why::KeptEnglish
        );
        assert_eq!(
            kept("สวัสดี", InputLayout::ThaiKedmanee, en, th),
            Why::KeptThai
        );
        assert_eq!(
            kept("qzxv", InputLayout::UsQwerty, en, th),
            Why::KeptUnknown
        );
        assert_eq!(fixed("name@gmail.com", true), Why::FixedAddress);
        assert_eq!(fixed("1,250", true), Why::FixedNumber);
        assert_eq!(fixed("สวัสดี", true), Why::FixedWord);
        assert_eq!(fixed("สวัสดีครับ", false), Why::FixedWords);
        for (i, w) in Why::ALL.iter().enumerate() {
            assert_eq!(Why::from_u8(i as u8), *w);
        }
    }
}
