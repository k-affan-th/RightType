//! Wrong-layout detection with Dvorak and Colemak as the English keyboard.
//! The English variant is process-wide state, so these tests take a lock.

use std::sync::Mutex;

use righttype::dict;
use righttype::find::Query;
use righttype::layout::{self, set_english_variant, EnglishVariant};
use righttype::policy::{detect_token, InputLayout};

static LOCK: Mutex<()> = Mutex::new(());

fn with_english(variant: EnglishVariant, f: impl FnOnce()) {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    set_english_variant(variant);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    set_english_variant(EnglishVariant::Us);
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

fn check(variant: EnglishVariant) {
    with_english(variant, || {
        let (en, th) = (dict::english(), dict::thai());

        // Thai typed with the English keyboard on: the characters are the
        // variant's, not QWERTY's, and still come back as the Thai word.
        let typed = layout::th_to_en("สวัสดี");
        set_english_variant(EnglishVariant::Us);
        assert_ne!(typed, layout::th_to_en("สวัสดี"), "{variant:?} must differ");
        set_english_variant(variant);
        let d = detect_token(&typed, InputLayout::UsQwerty, en, th)
            .unwrap_or_else(|| panic!("{variant:?}: {typed} not detected"));
        assert_eq!(d.corrected, "สวัสดี", "{variant:?}");

        // English typed with the Thai keyboard on comes back as English.
        let typed = layout::en_to_th("hello");
        let d = detect_token(&typed, InputLayout::ThaiKedmanee, en, th)
            .unwrap_or_else(|| panic!("{variant:?}: {typed} not detected"));
        assert_eq!(d.corrected, "hello", "{variant:?}");

        // A correct English word on this keyboard is left alone.
        assert!(detect_token("hello", InputLayout::UsQwerty, en, th).is_none());

        // Search typed on the wrong keyboard follows the variant's keys.
        let keys = layout::th_to_en("ลูกศร");
        assert!(
            Query::new(&keys).score("ลูกศรชี้ขวา").is_some(),
            "{variant:?}: {keys}"
        );
    });
}

#[test]
fn dvorak_is_detected_both_ways() {
    check(EnglishVariant::Dvorak);
}

#[test]
fn colemak_is_detected_both_ways() {
    check(EnglishVariant::Colemak);
}
