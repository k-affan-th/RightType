//! The opt-in reading of short number-row text typed on the Thai keyboard.
//! Its own test binary: the setting is process-wide.

use righttype::dict;
use righttype::layout::en_to_th;
use righttype::policy::{self, InputLayout};

fn read(typed: &str) -> Option<String> {
    policy::detect_token(
        typed,
        InputLayout::ThaiKedmanee,
        dict::english(),
        dict::thai(),
    )
    .map(|d| d.corrected)
}

#[test]
fn short_numbers_are_read_only_when_the_setting_is_on() {
    let samples = ["32", "3", "5", "1", "12", "456"];
    // Off (the default): nothing changes.
    assert!(!policy::reads_short_numbers());
    for n in &samples[..3] {
        assert_eq!(read(&en_to_th(n)), None, "off: {n}");
    }
    assert_eq!(en_to_th("32"), "-/");

    policy::set_reads_short_numbers(true);
    for n in samples {
        assert_eq!(read(&en_to_th(n)).as_deref(), Some(n), "on: {n}");
    }
    // Thai words stay.
    for word in ["ภาค", "จุด", "ที่", "และ"] {
        assert_eq!(read(word), None, "word: {word}");
    }
    policy::set_reads_short_numbers(false);
    assert_eq!(read(&en_to_th("32")), None);
}
