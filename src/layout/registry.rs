//! Which language and table a Windows keyboard handle (HKL) is.
//!
//! The OS-free half of keyboard detection: the Windows layer reads what only
//! Windows can say (which installed keyboard uses which layout file) and
//! hands it over as [`KeyboardFacts`]; everything else is decided here, so a
//! new language is a rule in [`detect_with`], its tables and a dictionary.
//!
//! A keyboard that cannot be told is `None`: RightType stays off for it
//! rather than read its keys through a table that may be wrong.

use std::sync::RwLock;

use super::LayoutId;

/// A language RightType converts between.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Language {
    Thai,
    English,
}

/// A keyboard that was recognised.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Detected {
    pub language: Language,
    pub table: LayoutId,
}

/// What the installed keyboards are, by the high word of their handle: the
/// `0xF000 +` "Layout Id" of a keyboard installed from a file, or the
/// keyboard's own identifier.
#[derive(Clone, Default, Debug)]
pub struct KeyboardFacts {
    pub thai: Vec<(u16, LayoutId)>,
    pub english: Vec<(u16, LayoutId)>,
}

static FACTS: RwLock<KeyboardFacts> = RwLock::new(KeyboardFacts {
    thai: Vec::new(),
    english: Vec::new(),
});

/// Tell the registry what the installed keyboards are.
pub fn set_facts(facts: KeyboardFacts) {
    if let Ok(mut f) = FACTS.write() {
        *f = facts;
    }
}

/// The table of a keyboard handle, from the facts last set.
pub fn detect(hkl: u32) -> Option<Detected> {
    FACTS.read().ok().and_then(|f| detect_with(hkl, &f))
}

/// The table of a keyboard's layout file (`Layout File` and `Layout Text` in
/// the registry), for the language the keyboard is installed for.
///
/// Only keyboards whose keys sit where a bundled table puts them count:
/// US (`KBDUS`), UK (`KBDUK`), US Dvorak (`KBDDV`; the one-handed ones are
/// different), Colemak (not its DH variants), Thai `KBDTH0`–`3` and
/// Manoonchai.
pub fn classify_layout_file(language: Language, file: &str, text: &str) -> Option<LayoutId> {
    let file = file.to_ascii_uppercase();
    let stem = file.strip_suffix(".DLL").unwrap_or(&file);
    let text = text.to_ascii_uppercase();
    match language {
        Language::Thai => match stem {
            "KBDTH0" | "KBDTH2" => Some(LayoutId::Kedmanee),
            "KBDTH1" | "KBDTH3" => Some(LayoutId::Pattachote),
            _ if stem.contains("MANOON") || text.contains("MANOONCHAI") => {
                Some(LayoutId::Manoonchai)
            }
            _ => None,
        },
        Language::English => match stem {
            "KBDUS" => Some(LayoutId::QwertyEn),
            "KBDUK" => Some(LayoutId::QwertyUk),
            "KBDDV" => Some(LayoutId::DvorakEn),
            _ if text.contains("COLEMAK") && !text.contains("DH") && !text.contains("MOD") => {
                Some(LayoutId::ColemakEn)
            }
            _ => None,
        },
    }
}

/// Of the keyboards installed, the one to start with: `preferred` (the one
/// last used) when it is installed, else the first.
pub fn prefer<T: PartialEq + Copy>(installed: &[T], preferred: T) -> Option<T> {
    installed
        .iter()
        .copied()
        .find(|t| *t == preferred)
        .or_else(|| installed.first().copied())
}

/// Whether Thai keyboards are installed but none of them is one RightType can
/// read: converting into Thai would then go through a table that is not the
/// typist's.
pub fn thai_unreadable(handles: &[u32], facts: &KeyboardFacts) -> bool {
    let thai = |h: &&u32| *h & 0x3FF == 0x1E;
    let any = handles.iter().any(|h| *h & 0x3FF == 0x1E);
    any && !handles
        .iter()
        .filter(thai)
        .any(|h| detect_with(*h, facts).is_some())
}

/// [`detect`] against explicit facts.
///
/// The low word of the handle is the language, the high word the keyboard
/// (zero, or equal to the language, for the language's default keyboard).
pub fn detect_with(hkl: u32, facts: &KeyboardFacts) -> Option<Detected> {
    let language = hkl & 0xFFFF;
    let device = hkl >> 16;
    let lookup = |list: &[(u16, LayoutId)]| {
        list.iter()
            .find(|(d, _)| u32::from(*d) == device)
            .map(|(_, t)| *t)
    };
    match language & 0x3FF {
        0x1E => {
            let table = if device == 0 || device == language {
                Some(LayoutId::Kedmanee)
            } else {
                lookup(&facts.thai)
            };
            table.map(|table| Detected {
                language: Language::Thai,
                table,
            })
        }
        0x09 => {
            let keyboard = if device == 0 { language } else { device };
            let table = match keyboard {
                // Australia, New Zealand and the US itself.
                0x0409 | 0x0C09 | 0x1409 => Some(LayoutId::QwertyEn),
                0x0809 => Some(LayoutId::QwertyUk),
                _ => lookup(&facts.english),
            };
            table.map(|table| Detected {
                language: Language::English,
                table,
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_thai() {
        let c = |f, t| classify_layout_file(Language::Thai, f, t);
        assert_eq!(c("KBDTH0.DLL", ""), Some(LayoutId::Kedmanee));
        assert_eq!(c("kbdth2.dll", ""), Some(LayoutId::Kedmanee));
        assert_eq!(c("KBDTH1.DLL", ""), Some(LayoutId::Pattachote));
        assert_eq!(c("KBDTH3.DLL", ""), Some(LayoutId::Pattachote));
        assert_eq!(c("kbdx.dll", "Thai Manoonchai"), Some(LayoutId::Manoonchai));
        assert_eq!(c("KBDUS.DLL", ""), None);
    }

    #[test]
    fn classify_english() {
        let c = |f, t| classify_layout_file(Language::English, f, t);
        assert_eq!(c("KBDUS.DLL", "US"), Some(LayoutId::QwertyEn));
        assert_eq!(c("KBDUK.DLL", "United Kingdom"), Some(LayoutId::QwertyUk));
        assert_eq!(
            c("KBDDV.DLL", "United States-Dvorak"),
            Some(LayoutId::DvorakEn)
        );
        assert_eq!(c("KBDUSL.DLL", "US-Dvorak for left hand"), None);
        assert_eq!(c("KBDUSR.DLL", "US-Dvorak for right hand"), None);
        assert_eq!(c("kbdcol.dll", "Colemak"), Some(LayoutId::ColemakEn));
        assert_eq!(c("kbdcol.dll", "Colemak-DH"), None);
        assert_eq!(c("KBDGR.DLL", "German"), None);
    }

    #[test]
    fn detect_defaults() {
        let none = KeyboardFacts::default();
        let d = |h| detect_with(h, &none).map(|d| d.table);
        for h in [0x0409_0409, 0x0000_0409, 0x0C09_0C09, 0x1409_1409] {
            assert_eq!(d(h), Some(LayoutId::QwertyEn), "{h:#x}");
        }
        assert_eq!(d(0x0809_0809), Some(LayoutId::QwertyUk));
        assert_eq!(d(0x0409_1009), Some(LayoutId::QwertyEn));
        assert_eq!(d(0x1009_1009), None);
        assert_eq!(d(0x041E_041E), Some(LayoutId::Kedmanee));
        assert_eq!(d(0x0000_041E), Some(LayoutId::Kedmanee));
        assert_eq!(d(0x0407_0407), None);
        assert_eq!(d(0xF00F_041E), None);
        assert_eq!(d(0xF002_0409), None);
    }

    #[test]
    fn prefer_keeps_the_one_last_used() {
        assert_eq!(prefer(&[1, 2, 3], 2), Some(2));
        assert_eq!(prefer(&[1, 3], 2), Some(1));
        assert_eq!(prefer::<u8>(&[], 2), None);
    }

    #[test]
    fn unreadable_thai_is_only_when_no_thai_keyboard_is_known() {
        let none = KeyboardFacts::default();
        // Only an unknown Thai keyboard: unreadable.
        assert!(thai_unreadable(&[0xF00F_041E, 0x0409_0409], &none));
        // The default Thai keyboard is also there: readable.
        assert!(!thai_unreadable(&[0xF00F_041E, 0x041E_041E], &none));
        // No Thai keyboard at all is not "unreadable".
        assert!(!thai_unreadable(&[0x0409_0409], &none));
    }

    #[test]
    fn detect_facts() {
        let facts = KeyboardFacts {
            thai: vec![(0xF001, LayoutId::Pattachote)],
            english: vec![(0xF010, LayoutId::DvorakEn)],
        };
        let d = |h| detect_with(h, &facts);
        assert_eq!(
            d(0xF001_041E),
            Some(Detected {
                language: Language::Thai,
                table: LayoutId::Pattachote
            })
        );
        assert_eq!(d(0xF010_0409).map(|d| d.table), Some(LayoutId::DvorakEn));
        assert_eq!(d(0xF010_041E), None);
    }
}
