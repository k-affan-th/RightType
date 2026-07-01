use righttype::buffer::{Key, WordBuffer};
use righttype::detect;
use righttype::dict;
use righttype::layout::{en_to_th, th_to_en, auto_convert};
use righttype::secret::{self, SeedTracker};
use righttype::segment;

#[test]
fn test_layout_conversion_basics() {
    // English QWERTY to Thai Kedmanee
    assert_eq!(en_to_th("correct"), "แนพพำแะ");
    assert_eq!(en_to_th("hello"), "\u{0e49}\u{0e33}\u{0e2a}\u{0e2a}\u{0e19}");
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
    assert!(secret::is_secret_token("aA1!bB2#"));    // Mixed classes
    assert!(secret::is_secret_token("zX9#mLq!"));    // Mixed classes

    // 2. High-entropy strings
    assert!(secret::is_secret_token("4f8a9c2b1d0e")); // Hex private key fragment
    
    // 3. Cryptographic addresses and keys
    assert!(secret::is_secret_token("bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq")); // Bech32
    assert!(secret::is_secret_token("5HueCGU8rMjxEXxiPuD5BDku4MkFqeZyd4dZ1jvhTVqvbTLvyTJ")); // WIF
    assert!(secret::is_secret_token("xpub661MyMwAqRbcFtXgS5sYJABqqG9YLmC4Q1Rdap9gSE8NqtwybGhePY2gZ29ESFjqJoCu1Rupje8YtGqsefD265TMg7usUDFdp6W1EGMcet")); // xpub

    // 4. Ordinary long words (TooLong)
    let very_long_word = "a".repeat(25);
    assert!(secret::is_secret_token(&very_long_word));

    // 5. Ordinary words that should NOT be classified as secrets
    assert!(!secret::is_secret_token("hello"));
    assert!(!secret::is_secret_token("password")); // only lowercase letters (1 class, low entropy)
    assert!(!secret::is_secret_token("สวัสดี"));  // Thai script is never secret
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
fn test_live_conversion_qwerty_to_thai() {
    let dict_th = dict::thai();
    
    // Simulate typing "สวัสดี" on QWERTY layout: "l;ylfu"
    // Since QWERTY layout is active, we check en_to_th conversion.
    // We simulate key-by-key input.
    let mut input = String::new();
    let keys = vec!['l', ';', 'y', 'l', 'f', 'u'];
    
    let mut triggered = false;
    let mut corrected_text = String::new();

    for c in keys {
        input.push(c);
        
        // Simulating the logic inside auto_en_layout_to_thai():
        let current = input.clone();
        let n = current.chars().count();
        
        if n >= 2 
            && current.is_ascii() 
            && !secret::is_secret_token(&current)
            && !dict::english().contains(&current)
        {
            let thai = en_to_th(&current);
            if segment::is_fully_known(&thai, dict_th) {
                triggered = true;
                corrected_text = thai;
                break;
            }
        }
    }

    // It should trigger at "l;" (which converts to "สว", a valid Thai word).
    assert!(triggered);
    assert_eq!(corrected_text, "สว");
}

#[test]
fn test_live_conversion_thai_to_qwerty() {
    let dict_en = dict::english();
    
    // Simulate typing "correct" on Thai layout: "แนพพำแะ"
    // Since Thai layout is active, we check th_to_en conversion.
    // Live conversion requires length >= 4.
    let mut input = String::new();
    let keys = vec!['แ', 'น', 'พ', 'พ', 'ำ', 'แ', 'ะ'];
    
    let mut triggered = false;
    let mut corrected_text = String::new();

    for c in keys {
        input.push(c);
        
        // Simulating the logic inside auto_thai_layout_to_en():
        let current = input.clone();
        let n = current.chars().count();
        
        if n >= 4
            && !current.is_ascii()
            && !secret::is_secret_token(&current)
            && !dict::thai().contains(&current)
        {
            let eng = th_to_en(&current);
            if dict_en.contains(&eng) {
                triggered = true;
                corrected_text = eng;
                break;
            }
        }
    }

    // "correct" should trigger at length 4 ("แนพพ" -> "corr", which is a valid word).
    assert!(triggered);
    assert_eq!(corrected_text, "corr");

    // Let's test a shorter word like "the" typed on Thai layout: "ะ้ำ" (3 chars)
    // It should NOT trigger live conversion because n < 4.
    let mut short_input = String::new();
    let short_keys = vec!['ะ', '้', 'ำ']; // "the"
    let mut short_triggered = false;

    for c in short_keys {
        short_input.push(c);
        let current = short_input.clone();
        let n = current.chars().count();
        if n >= 4 {
            let eng = th_to_en(&current);
            if dict_en.contains(&eng) {
                short_triggered = true;
                break;
            }
        }
    }
    assert!(!short_triggered);
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
    assert!(!tracker.observe("able"));    // run = 3
    assert!(tracker.observe("about"));     // run = 4 -> trips!

    // 2. Resets on non-BIP39 word
    tracker.reset();
    assert!(!tracker.observe("abandon")); // run = 1
    assert!(!tracker.observe("hello"));   // run = 0 -> resets!
    assert!(!tracker.observe("ability")); // run = 1
}

#[test]
fn test_bip39_seed_tracker_wrong_layout_fails_to_trip() {
    let mut tracker = SeedTracker::new();

    // Verify the edge case: if a user types a seed phrase in the wrong layout (Thai),
    // the seed tracker does NOT trip on the raw typed word because it checks ASCII.
    // e.g. "abandon" typed on Thai layout: "ฟิฟืกนด"
    // "ability" typed on Thai layout: "ฟิิสระั"
    // "able" typed on Thai layout: "ฟิกสำ"
    // "about" typed on Thai layout: "ฟินีะ"
    
    assert!(!tracker.observe("ฟิฟืกนด")); // resets or stays at 0 because it's Thai
    assert!(!tracker.observe("ฟิิสระั")); // run = 0
    assert!(!tracker.observe("ฟิกสำ"));   // run = 0
    assert!(!tracker.observe("ฟินีะ"));   // run = 0 -> does NOT trip!
}

#[test]
fn test_word_buffer_poisoning_and_backspace() {
    // WordBuffer capacity is 24 chars by default.
    let mut buf = WordBuffer::new();

    // 1. Type 24 characters (exactly at cap)
    for _ in 0..24 {
        buf.observe(Key::Char('a'));
    }
    assert_eq!(buf.current().len(), 24);

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
    assert_eq!(delete_count, 3);      // 2 chars for "ok" + 1 for space
}

#[test]
fn test_antigravity_prefixes() {
    let dict_th = dict::thai();
    for word in &["Antigravity", "fucking", "idiot"] {
        println!("--- Word: {} ---", word);
        for i in 1..=word.len() {
            let prefix = &word[0..i];
            let thai = en_to_th(prefix);
            let fully_known = segment::is_fully_known(&thai, dict_th);
            println!("Prefix: {}, Thai: {}, Fully Known: {}", prefix, thai, fully_known);
        }
    }
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
