use righttype::buffer::{Key, WordBuffer};
use righttype::detect;
use righttype::dict;
use righttype::layout::{auto_convert, en_to_th, th_to_en};
use righttype::policy::{self, InputLayout, Reading};
use righttype::render;
use righttype::secret::{self, SeedTracker};
use righttype::segment;

#[test]
fn test_layout_conversion_basics() {
    // English QWERTY to Thai Kedmanee
    assert_eq!(en_to_th("correct"), "แนพพำแะ");
    assert_eq!(
        en_to_th("hello"),
        "\u{0e49}\u{0e33}\u{0e2a}\u{0e2a}\u{0e19}"
    );
    assert_eq!(en_to_th("what"), "ไ้ฟะ");
    assert_eq!(en_to_th("twitter"), "ะไระะำพ");

    // Thai Kedmanee to English QWERTY
    assert_eq!(th_to_en("แนพพำแะ"), "correct");
    assert_eq!(th_to_en("สวัสดี"), "l;ylfu");
    assert_eq!(th_to_en("ไ้ฟะ"), "what");
}

#[test]
fn test_segmentation_edge_cases() {
    let dict_th = dict::thai();

    // 1. Normal sentence
    let segs = segment::segment("สวัสดีครับวันนี้วันจันทร์", dict_th);
    // Ensure it segments into known words
    assert!(segment::is_fully_known("สวัสดีครับวันนี้วันจันทร์", dict_th));

    // Check that all segments are known
    for seg in &segs {
        assert!(seg.known, "Segment {:?} should be known", seg);
    }

    // 2. Sentence with unknown garbage at the end
    let text_with_garbage = "สวัสดีครับวันนี้วันจันทร์xyz";
    assert!(!segment::is_fully_known(text_with_garbage, dict_th));
    let segs_g = segment::segment(text_with_garbage, dict_th);
    assert_eq!(segs_g.last().unwrap().text, "xyz");
    assert!(!segs_g.last().unwrap().known);

    // 3. Very short words (under MIN_WORD_CHARS = 2) should not be treated as known individual segments
    let text_single = "xyzกabc";
    let segs_s = segment::segment(text_single, dict_th);
    assert_eq!(segs_s.len(), 1);
    assert!(!segs_s[0].known);
}

#[test]
fn test_secret_entropy_detection() {
    // 1. Passwords with mixed character classes (should be detected as secret)
    assert!(secret::is_secret_token("P@ssw0rd123")); // Upper, lower, digit, symbol
    assert!(secret::is_secret_token("aA1!bB2#")); // Mixed classes
    assert!(secret::is_secret_token("zX9#mLq!")); // Mixed classes

    // 2. High-entropy strings
    assert!(secret::is_secret_token("4f8a9c2b1d0e")); // Hex private key fragment

    // 3. Cryptographic addresses and keys
    assert!(secret::is_secret_token(
        "bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq"
    )); // Bech32
    assert!(secret::is_secret_token(
        "5HueCGU8rMjxEXxiPuD5BDku4MkFqeZyd4dZ1jvhTVqvbTLvyTJ"
    )); // WIF
    assert!(secret::is_secret_token("xpub661MyMwAqRbcFtXgS5sYJABqqG9YLmC4Q1Rdap9gSE8NqtwybGhePY2gZ29ESFjqJoCu1Rupje8YtGqsefD265TMg7usUDFdp6W1EGMcet")); // xpub

    // 4. Ordinary long words (TooLong)
    let very_long_word = "a".repeat(25);
    assert!(secret::is_secret_token(&very_long_word));

    // 5. Ordinary words that should NOT be classified as secrets
    assert!(!secret::is_secret_token("hello"));
    assert!(!secret::is_secret_token("password")); // only lowercase letters (1 class, low entropy)
    assert!(!secret::is_secret_token("สวัสดี")); // Thai script is never secret
}

#[test]
fn test_url_and_email_behavior() {
    let dict_en = dict::english();
    let dict_th = dict::thai();

    // Emails and URLs contain punctuation, which are NOT split by WordBuffer.
    // They are evaluated as a single token.
    let email = "john@example.com";
    let url = "https://google.com";

    // 1. Should be classified as secret due to entropy / multiple classes if ASCII
    assert!(secret::is_secret_token(email));
    assert!(secret::is_secret_token(url));

    // 2. Even if they weren't classified as secret, they are not in the dictionary,
    // so they would not be corrected anyway.
    assert!(!dict_en.contains(email));
    assert!(!dict_en.contains(url));

    // 3. What if typed in the wrong layout (Thai)?
    // E.g. "john@example.com" typed on Thai layout: "่น้ื๑ำปฟทสแณท"
    let wrong_email = en_to_th(email);
    // Since it contains Thai characters, it is NOT ASCII and thus NOT classified as secret.
    assert!(!secret::is_secret_token(&wrong_email));

    // But it converts to "john@example.com", which is NOT in the English dictionary.
    // So detect should return None.
    let det = detect::detect(&wrong_email, dict_en, dict_th);
    assert!(det.is_none());
}

#[test]
fn test_qwerty_to_thai_is_detected_at_the_boundary() {
    // EN→Thai deliberately waits for a word boundary: live conversion of a two
    // character prefix such as "l;" would make ordinary English typing unsafe.
    let det = policy::detect_token(
        "l;ylfu",
        InputLayout::UsQwerty,
        dict::english(),
        dict::thai(),
    )
    .unwrap();
    assert_eq!(det.corrected, "สวัสดี");
}

#[test]
fn test_thai_to_qwerty_waits_for_boundary() {
    let mut buffer = WordBuffer::new();
    for c in "แนพพำแะ".chars() {
        assert!(buffer.observe(Key::Char(c)).is_none());
    }
    let completed = buffer.observe(Key::Boundary).unwrap();
    let det = policy::detect_token(
        &completed,
        InputLayout::ThaiKedmanee,
        dict::english(),
        dict::thai(),
    )
    .unwrap();
    assert_eq!(det.corrected, "correct");
}

#[test]
fn test_boundary_conversion() {
    let dict_en = dict::english();
    let dict_th = dict::thai();

    // A short word like "the" (3 chars) typed on Thai layout ("ะ้ำ") doesn't trigger live.
    // But it SHOULD trigger when a boundary (space) is pressed.
    // detect::detect handles boundary conversions.
    let input = "ะ้ำ";
    let det = detect::detect(input, dict_en, dict_th).unwrap();
    assert_eq!(det.corrected, "the");
    assert_eq!(det.confidence, detect::Confidence::High);

    // Typing a short Thai word like "กับ" (3 chars) on QWERTY layout ("dy[")
    let input_qwerty = "dy[";
    let det_qwerty = detect::detect(input_qwerty, dict_en, dict_th).unwrap();
    assert_eq!(det_qwerty.corrected, "กับ");
}

#[test]
fn test_bip39_seed_tracker_straight_ascii() {
    let mut tracker = SeedTracker::new();

    // 1. BIP39 sequence
    assert!(!tracker.observe("abandon")); // run = 1
    assert!(!tracker.observe("ability")); // run = 2
    assert!(!tracker.observe("able")); // run = 3
    assert!(tracker.observe("about")); // run = 4 -> trips!

    // 2. Resets on non-BIP39 word
    tracker.reset();
    assert!(!tracker.observe("abandon")); // run = 1
    assert!(!tracker.observe("hello")); // run = 0 -> resets!
    assert!(!tracker.observe("ability")); // run = 1
}

#[test]
fn test_bip39_seed_tracker_uses_wrong_layout_candidate() {
    let mut tracker = SeedTracker::new();
    // The production stream guard observes the English candidate, not only the
    // Thai glyphs emitted by the wrong layout. It trips at the documented fourth
    // word, while remaining deliberately non-retrospective.
    for (index, candidate) in ["abandon", "ability", "able", "about"].iter().enumerate() {
        let raw = en_to_th(candidate);
        assert_eq!(tracker.observe_candidate(&raw, Some(candidate)), index == 3);
    }
}

#[test]
fn test_word_buffer_poisoning_and_backspace() {
    // The default cap admits a Thai sentence typed without spaces on QWERTY.
    let mut buf = WordBuffer::new();

    // 1. Type exactly the default cap.
    for _ in 0..righttype::buffer::MAX_BUFFER_CHARS {
        buf.observe(Key::Char('a'));
    }
    assert_eq!(buf.current().len(), righttype::buffer::MAX_BUFFER_CHARS);

    // 2. Type 1 more character -> exceeds cap, poisons and clears immediately
    buf.observe(Key::Char('a'));
    assert_eq!(buf.current(), "");

    // 3. Backspacing on a poisoned buffer should have no effect
    buf.observe(Key::Backspace);
    assert_eq!(buf.current(), "");

    // 4. Typing more characters keeps being ignored
    buf.observe(Key::Char('b'));
    assert_eq!(buf.current(), "");

    // 5. Boundary resets poison state and returns None (since poisoned completed word is ignored)
    let completed = buf.observe(Key::Boundary);
    assert!(completed.is_none());
    assert_eq!(buf.current(), "");

    // 6. Buffer is clean and usable again
    buf.observe(Key::Char('x'));
    assert_eq!(buf.current(), "x");
}

#[test]
fn test_mixed_script_handling() {
    let dict_en = dict::english();
    let dict_th = dict::thai();

    // Mixed script word like "Helloสวัสดี"
    let mixed = "Helloสวัสดี";

    // detect::detect should ignore mixed scripts (returns None)
    let det = detect::detect(mixed, dict_en, dict_th);
    assert!(det.is_none());

    // ASCII mixed with Thai gibberish like "l;ylfuhello"
    // detect::detect should ignore it
    let mixed_gibberish = "l;ylfuhello";
    let det2 = detect::detect(mixed_gibberish, dict_en, dict_th);
    assert!(det2.is_none());
}

#[test]
fn test_proper_nouns_and_common_overlap() {
    let dict_en = dict::english();
    let dict_th = dict::thai();

    // Words that are correct in the active layout should not be corrected.
    // 1. English proper nouns / brand names in English dictionary (e.g. "twitter", "google")
    // When typed on English layout, they should remain as is.
    let det1 = detect::detect("twitter", dict_en, dict_th);
    assert!(det1.is_none());

    // 2. What if a word is valid in both layouts?
    // In our dictionary, English words are in dict_en, Thai words in dict_th.
    // If a user types "correct" (valid English word) on English layout, it is not corrected.
    let det2 = detect::detect("correct", dict_en, dict_th);
    assert!(det2.is_none());

    // If a user types "สวัสดี" (valid Thai word) on Thai layout, it is not corrected.
    let det3 = detect::detect("สวัสดี", dict_en, dict_th);
    assert!(det3.is_none());
}

#[test]
fn test_manual_convert_last_word_after_boundary() {
    // Simulating: typing "ok" (buffer has "ok"), pressing Space (buffer completed),
    // then manually converting the last completed word.
    let mut last_completed: Option<(String, u16)> = Some(("ok".to_string(), 0x20)); // 0x20 = VK_SPACE

    // Simulate convert_last_word logic when buffer is empty:
    let last = last_completed.take();
    let mut triggered = false;
    let mut corrected_text = String::new();
    let mut delete_count = 0;

    if let Some((last_word, boundary_vk)) = last {
        delete_count = last_word.chars().count() + 1;
        let converted = auto_convert(&last_word);
        if converted != last_word {
            triggered = true;
            corrected_text = converted;
        }
        assert_eq!(boundary_vk, 0x20);
    }

    assert!(triggered);
    assert_eq!(corrected_text, "นา"); // "ok" on Thai layout maps to "นา"
    assert_eq!(delete_count, 3); // 2 chars for "ok" + 1 for space
}

#[test]
fn test_antigravity_prefixes() {
    for word in ["Antigravity", "fucking", "idiot", "correct", "something"] {
        let mut buffer = WordBuffer::new();
        for c in word.chars() {
            assert!(
                buffer.observe(Key::Char(c)).is_none(),
                "{word} converted mid-word"
            );
        }
        let completed = buffer.observe(Key::Boundary).unwrap();
        assert!(policy::detect_token(
            &completed,
            InputLayout::UsQwerty,
            dict::english(),
            dict::thai(),
        )
        .is_none());
    }
}

#[test]
fn test_code_commands_and_paths_do_not_auto_convert() {
    for token in [
        "cargo",
        "kubectl",
        "println!",
        "main()",
        "--release",
        "git-status",
        "C:\\Windows\\System32",
        "SELECT*FROM",
        "user@example.com",
        "https://example.com",
    ] {
        assert!(
            policy::detect_token(token, InputLayout::UsQwerty, dict::english(), dict::thai(),)
                .is_none(),
            "code/command token converted: {token}"
        );
    }
}

#[test]
fn test_no_space_thai_segmentation_examples() {
    // Natural demo for users.
    let natural = "สวัสดีครับวันนี้";
    let natural_raw = th_to_en(natural);
    let natural_detection = policy::detect_token(
        &natural_raw,
        InputLayout::UsQwerty,
        dict::english(),
        dict::thai(),
    )
    .unwrap();
    assert_eq!(natural_detection.corrected, natural);

    // "สวัสดีดี" is intentionally awkward Thai, but it is a useful regression
    // for repeated-word/full-segmentation behavior rather than the best UX demo.
    let stress = "สวัสดีดี";
    let stress_raw = th_to_en(stress);
    let stress_detection = policy::detect_token(
        &stress_raw,
        InputLayout::UsQwerty,
        dict::english(),
        dict::thai(),
    )
    .unwrap();
    assert_eq!(stress_detection.corrected, stress);
}

#[test]
fn test_fucking_idiot_conversion() {
    let dict_en = dict::english();
    let dict_th = dict::thai();

    let th_word = "รกรนะ";
    assert!(!dict_th.contains(th_word));
    let en_word = th_to_en(th_word);
    assert_eq!(en_word, "idiot");
    assert!(dict_en.contains(&en_word));
}

#[test]
fn test_thai_on_english_layout_with_symbols() {
    let cases = [
        ("0ib'vp^jmuj;jklk,ki5cx]'d]y[wfh", "จริงอยู่ที่ว่าสามารถแปลงกลับได้"),
        (
            "c9j-hv8;k,ouhpy'w,jcx]'.sh9yh'c9jcidfh;p:he",
            "แต่ข้อความนี้ยังไม่แปลงให้ตั้งแต่แรกด้วยซ้ำ",
        ),
        ("clf';jk,uvtwime'kozbf", "แสดงว่ามีอะไรทำงานผิด"),
    ];

    for (raw, expected) in cases {
        // These are intentionally secret-shaped ASCII strings because Thai
        // layout keys include digits and punctuation. The valid full Thai
        // segmentation is the stronger signal.
        assert!(secret::is_secret_token(raw));

        let mut buffer = WordBuffer::new();
        for c in raw.chars() {
            buffer.observe(Key::Char(c));
        }
        let completed = buffer.observe(Key::Boundary).unwrap();
        let det = detect::detect(&completed, dict::english(), dict::thai()).unwrap();
        assert_eq!(det.corrected, expected);
    }
}

// ---------------------------------------------------------------------------
// D-006 live path: the blind-typist contract.
//
// Auto mode exists so a typist never has to look at the screen or switch
// language. That promise is symmetric, and both halves are asserted here:
// Thai typed on the English layout must come out as the intended Thai, and
// English typed on the English layout must come out completely untouched.
// ---------------------------------------------------------------------------

/// Replay the production D-006 live decision for one in-flight token, in the
/// same order `hook.rs` does it: length gate, production detection policy,
/// live decision.
fn live_decision_for(pending: &str) -> policy::LiveDecision {
    if pending.chars().count() < policy::MIN_LIVE_COMMIT_CHARS {
        return policy::LiveDecision::None;
    }
    let en = dict::english();
    let th = dict::thai();
    match policy::detect_token(pending, InputLayout::UsQwerty, en, th) {
        Some(d) => policy::live_decision(Some(InputLayout::UsQwerty), pending, &d, en),
        None => policy::LiveDecision::None,
    }
}

/// What ends up on screen when `keys` are typed in Auto mode while the US
/// layout is active and nobody is watching. The first live commit replaces the
/// in-flight token and switches the layout to Thai, so every keystroke after it
/// produces Thai natively.
fn auto_mode_screen(keys: &str) -> String {
    let mut screen = String::new();
    let mut buf = String::new();
    let mut switched_to_thai = false;

    for k in keys.chars() {
        if switched_to_thai {
            screen.push_str(&en_to_th(&k.to_string()));
            continue;
        }
        screen.push(k);
        buf.push(k);
        if live_decision_for(&buf) == policy::LiveDecision::Commit {
            let corrected =
                policy::detect_token(&buf, InputLayout::UsQwerty, dict::english(), dict::thai())
                    .expect("a Commit decision implies a detection")
                    .corrected;
            for _ in 0..buf.chars().count() {
                screen.pop();
            }
            screen.push_str(&corrected);
            buf.clear();
            switched_to_thai = true;
        }
    }
    screen
}

#[test]
fn english_typed_on_the_english_layout_is_never_eaten_mid_word() {
    // Every one of these is an ordinary English word whose *prefix* converts to
    // valid Thai. The completed-word guard in `detect` cannot see them, because
    // a prefix is not a word.
    for word in [
        "different",
        "order",
        "women",
        "computer",
        "difficult",
        "write",
        "walking",
        "immediately",
        "mistake",
        "during",
        "middle",
        "street",
        "strong",
        "decision",
        "murder",
        "giving",
        "moving",
        "suddenly",
    ] {
        assert_eq!(
            auto_mode_screen(word),
            word,
            "typing {word:?} on the English layout must leave it untouched"
        );
    }
}

#[test]
fn a_token_that_can_still_grow_into_english_is_held_not_committed() {
    // `diffe` -> `different`, `wri` -> `write`: a live English continuation
    // exists, so the only non-destructive answer is Ambiguous.
    assert_eq!(live_decision_for("diffe"), policy::LiveDecision::Ambiguous);
    assert_eq!(live_decision_for("wri"), policy::LiveDecision::Ambiguous);
    assert_eq!(live_decision_for("orde"), policy::LiveDecision::Ambiguous);
}

#[test]
fn thai_typed_on_the_english_layout_still_arrives_intact() {
    // The other half of the contract: holding ambiguous tokens must not cost
    // the Thai direction the thing D-006 was introduced for.
    for phrase in [
        "สวัสดีครับ",
        "วันนี้วันจันทร์",
        "ผมชอบกินข้าวผัด",
        "ขอบคุณมากครับ",
        "เดี๋ยวโทรกลับนะ",
        "ส่งไฟล์มาให้หน่อย",
        "กินข้าวยัง",
    ] {
        assert_eq!(
            auto_mode_screen(&th_to_en(phrase)),
            phrase,
            "Thai typed on the English layout must still be recovered"
        );
    }
}

// ---------------------------------------------------------------------------
// D-008 revisable rendering.
//
// A reading chosen from four characters of evidence stops being a verdict: the
// screen is reconciled with the run's best reading on every keystroke, so an
// early guess can be withdrawn. The tests below pin the commit horizon from
// both sides, because it is the one number the design trades off against.
// ---------------------------------------------------------------------------

/// Replay a run through the same ownership loop as `hook::reconcile_run`, and
/// return what ends up on screen. `horizon` is how long a Thai reading must
/// hold before the run is anchored and the layout takes over.
fn owned_run_screen(keys: &str, horizon: usize) -> String {
    let en = dict::english();
    let th = dict::thai();
    let mut run = String::new();
    let mut screen = String::new();
    let mut holding = false;
    let mut stable = 0usize;
    let mut anchored = false;

    for k in keys.chars() {
        if anchored {
            // Layout switched: the keystroke produces Thai natively.
            screen.push_str(&en_to_th(&k.to_string()));
            continue;
        }
        run.push(k);
        let reading = policy::live_reading(&run, holding, en, th);
        let target = match &reading {
            Reading::AsTyped => run.clone(),
            Reading::Thai(thai) => thai.clone(),
        };
        let delta = render::delta(&screen, &target);
        for _ in 0..delta.backspaces {
            screen.pop();
        }
        screen.push_str(&delta.insert);
        match reading {
            Reading::AsTyped => {
                holding = false;
                stable = 0;
            }
            Reading::Thai(_) => {
                holding = true;
                stable += 1;
                if stable >= horizon {
                    anchored = true;
                    run.clear();
                }
            }
        }
    }
    screen
}

#[test]
fn a_mistyped_english_word_is_put_back_when_the_thai_reading_dies() {
    // `adavnce` is a typo for `advance`: not a word, and not on its way to one,
    // so D-007's live-continuation guard cannot see it. Its first four
    // characters do convert to valid Thai.
    for typo in ["adavnce", "addvance", "adition"] {
        // Horizon 1 is the D-007 one-shot commit: cemented, unrecoverable.
        assert_ne!(
            owned_run_screen(typo, 1),
            typo,
            "{typo:?} should be mangled by a one-shot commit"
        );
        // With a revisable window the next keystroke kills the Thai reading and
        // the run converges on what was actually typed.
        assert_eq!(
            owned_run_screen(typo, policy::COMMIT_HORIZON),
            typo,
            "{typo:?} must be restored once the Thai reading dies"
        );
    }
}

#[test]
fn the_commit_horizon_is_short_enough_to_anchor_real_thai() {
    // Both of these leave the dictionary partway through (a loanword, a name).
    // Anchoring hands the rest to the Thai layout; never anchoring would
    // withdraw the whole run when viability finally dies.
    for phrase in ["เปิดแอปไลน์หน่อย", "ผมชื่ออัฟฟานครับ"]
    {
        assert_eq!(
            owned_run_screen(&th_to_en(phrase), policy::COMMIT_HORIZON),
            phrase,
            "{phrase:?} must survive with the production horizon"
        );
        assert_ne!(
            owned_run_screen(&th_to_en(phrase), usize::MAX),
            phrase,
            "{phrase:?} is the reason the horizon exists; without it the run is lost"
        );
    }
}

#[test]
fn revisable_rendering_keeps_both_directions_correct() {
    for word in ["different", "computer", "write", "walking", "immediately"] {
        assert_eq!(owned_run_screen(word, policy::COMMIT_HORIZON), word);
    }
    for phrase in ["สวัสดีครับ", "วันนี้วันจันทร์", "ประชุมตอนบ่ายสองโมง"]
    {
        assert_eq!(
            owned_run_screen(&th_to_en(phrase), policy::COMMIT_HORIZON),
            phrase
        );
    }
}
