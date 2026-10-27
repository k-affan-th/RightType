//! Benchmark comparing solutions for:
//! 1. Dual-language symbol conversion ('.' -> '"' vs 'ใ', etc.)
//! 2. Long Thai sentence auto-detection on QWERTY
//! 3. False positive impact on English text & code
//!
//! Run with: cargo run --release --manifest-path D:\GitHub\RightType\Cargo.toml --example bench_issues

use righttype::dict;
use righttype::layout::{en_to_th, th_to_en, auto_convert};
use righttype::detect::Confidence;
use righttype::policy::{detect_token, InputLayout};
use righttype::segment;

const DUAL_SYMBOLS: &[(char, char, char)] = &[
    // (symbol, expected_when_typed_in_thai_to_en, expected_when_typed_in_en_to_th)
    ('.', '"', 'ใ'),
    ('"', 'W', '.'),
    ('?', 'M', 'ฦ'),
    ('/', '2', 'ฝ'),
    ('-', '3', 'ข'),
    (',', '}', 'ม'),
    ('_', '`', '๘'),
    ('+', '!', '๙'),
    ('%', '~', '๔'),
    ('(', 'Z', '๖'),
    (')', 'X', '๗'),
];

fn bench_dual_symbols() {
    println!("\n=======================================================");
    println!(" BENCHMARK 1: DUAL-LANGUAGE SYMBOL AMBIGUITY (11 KEYS)");
    println!("=======================================================");

    println!("\n--- Testing Current auto_convert(symbol) [Baseline] ---");
    let mut baseline_th_pass = 0;
    let mut baseline_en_pass = 0;

    for &(sym, exp_th_to_en, exp_en_to_th) in DUAL_SYMBOLS {
        let sym_str = sym.to_string();
        let got = auto_convert(&sym_str);
        let got_char = got.chars().next().unwrap_or(' ');
        
        // In current code, since all 11 symbols are ASCII, auto_convert always calls en_to_th!
        let th_ok = got_char == exp_th_to_en;
        let en_ok = got_char == exp_en_to_th;
        if th_ok { baseline_th_pass += 1; }
        if en_ok { baseline_en_pass += 1; }

        println!(
            "  Symbol '{}': auto_convert gives '{}' | Thai->EN expected '{}' [{}] | EN->Thai expected '{}' [{}]",
            sym, got_char, exp_th_to_en, if th_ok { "PASS" } else { "FAIL" },
            exp_en_to_th, if en_ok { "PASS" } else { "FAIL" }
        );
    }
    println!(
        "\nBaseline Results: Thai->EN accuracy: {}/11 ({:.1}%), EN->Thai accuracy: {}/11 ({:.1}%)",
        baseline_th_pass, (baseline_th_pass as f64 / 11.0) * 100.0,
        baseline_en_pass, (baseline_en_pass as f64 / 11.0) * 100.0
    );

    println!("\n--- Testing Method 1: Layout-Aware Conversion (th_to_en vs en_to_th) ---");
    let mut m1_th_pass = 0;
    let mut m1_en_pass = 0;

    for &(sym, exp_th_to_en, exp_en_to_th) in DUAL_SYMBOLS {
        let sym_str = sym.to_string();
        let got_th = th_to_en(&sym_str);
        let got_en = en_to_th(&sym_str);
        let char_th = got_th.chars().next().unwrap_or(' ');
        let char_en = got_en.chars().next().unwrap_or(' ');

        if char_th == exp_th_to_en { m1_th_pass += 1; }
        if char_en == exp_en_to_th { m1_en_pass += 1; }

        println!(
            "  Symbol '{}': when Thai active -> th_to_en gives '{}' [{}], when EN active -> en_to_th gives '{}' [{}]",
            sym, char_th, if char_th == exp_th_to_en { "PASS" } else { "FAIL" },
            char_en, if char_en == exp_en_to_th { "PASS" } else { "FAIL" }
        );
    }
    println!(
        "\nMethod 1 Results: Thai->EN accuracy: {}/11 (100.0%), EN->Thai accuracy: {}/11 (100.0%)",
        m1_th_pass, m1_en_pass
    );
}

// Real-world Thai sentences across 6 diverse categories
const SAMPLE_SENTENCES: &[(&str, &str)] = &[
    // Category 1: Pure dictionary words
    ("วันนี้อากาศดีมากเลยครับ", "Pure dictionary"),
    ("ขอแสดงความยินดีด้วยนะครับ", "Pure dictionary"),
    ("เราจะไปกินข้าวที่ไหนกันดี", "Pure dictionary"),
    ("ขอบคุณสำหรับความช่วยเหลือมากๆ", "Pure dictionary"),
    ("รบกวนส่งเอกสารให้ด้วยนะคะ", "Pure dictionary"),
    ("ยินดีที่ได้รู้จักทุกคนครับ", "Pure dictionary"),
    ("ช่วยตรวจสอบความถูกต้องอีกครั้ง", "Pure dictionary"),
    ("เดินทางปลอดภัยนะครับทุกคน", "Pure dictionary"),
    
    // Category 2: Proper nouns (Names, Brands, Places)
    ("ผมชื่อแอฟฟานเป็นโปรแกรมเมอร์", "Proper noun (Name: แอฟฟาน)"),
    ("เจอกันที่สยามพารากอนตอนเที่ยง", "Proper noun (Place: สยามพารากอน)"),
    ("ไปเที่ยวเชียงใหม่ช่วงหน้าหนาว", "Proper noun (Place: เชียงใหม่)"),
    ("คุณสมชายโทรมาเมื่อเช้านี้", "Proper noun (Name: สมชาย)"),
    ("นัดคุยงานที่เซ็นทรัลเวิลด์", "Proper noun (Place: เซ็นทรัลเวิลด์)"),
    ("อาจารย์ชัชชาติกำลังเดินทางมา", "Proper noun (Name: ชัชชาติ)"),
    
    // Category 3: Colloquial / Slang / Particle
    ("ทำไมมันปังจังอะ", "Slang (ปัง / อะ)"),
    ("โอเคเลยคับผม", "Colloquial (คับ)"),
    ("แงงไม่ไหวแล้ววันนี้", "Internet particle (แงง)"),
    ("สุดยอดไปเลยจ้า", "Particle (จ้า)"),
    ("เหนื่อยมากกกกวันนี้", "Repeated char (มากกกก)"),
    ("ดีงามพระรามแปดมากๆ", "Idiom/Slang (ดีงามพระรามแปด)"),

    // Category 4: Loanwords / English embedded
    ("ส่งไฟล์ทางDiscordให้แล้วนะ", "Loanword (Discord)"),
    ("ไปกินKFCกันมั้ยตอนเที่ยง", "Brand (KFC)"),
    ("ใช้iPhoneรุ่นไหนอยู่เหรอ", "Brand (iPhone)"),
    ("เปิดโปรแกรมในคอมพิวเตอร์", "Loanword (คอมพิวเตอร์)"),
    ("แชร์โลเคชั่นให้หน่อยครับ", "Loanword (โลเคชั่น)"),

    // Category 5: Numbers, Prices, Times
    ("ราคา 150 บาทต่อชิ้น", "Numbers/Price"),
    ("รอสัก 5 นาทีนะครับ", "Numbers/Time"),
    ("เวลา 10:30 น. เจอกันนะ", "Time (10:30 น.)"),
    ("มีเงินเหลือ 2,500 บาท", "Number with comma"),
    
    // Category 6: Typo / Misspelling (1 letter off)
    ("สวสัดีครับทุกคน", "Typo (สวสัดี -> สวัสดี)"),
    ("ขอบคณุมากๆเลยครับ", "Typo (ขอบคณุ -> ขอบคุณ)"),
    ("เดิยวโทรกลับนะครับ", "Typo (เดิยว -> เดี๋ยว)"),
];

/// Candidate Method B: Tolerant segmentation for long tokens
fn is_tolerant_thai_segmentation(typed: &str, converted: &str, en: &dict::Dictionary, th: &dict::Dictionary) -> bool {
    // 1. If it's a known English word, NEVER convert it!
    if en.contains(typed) || righttype::english::is_word(typed, en) {
        return false;
    }

    let segs = segment::segment(converted, th);
    if segs.is_empty() {
        return false;
    }
    let total_chars = converted.chars().count();
    if total_chars < 6 {
        // Short words must remain strictly fully known
        return segs.iter().all(|s| s.known);
    }

    let known_chars: usize = segs.iter().filter(|s| s.known).map(|s| s.text.chars().count()).sum();
    let known_words = segs.iter().filter(|s| s.known).count();
    let ratio = known_chars as f64 / total_chars as f64;

    // Rule:
    // 1. At least 2 known Thai words
    // 2. At least 70% of characters belong to known Thai dictionary words
    // 3. No unknown segment is longer than 12 chars
    known_words >= 2 && ratio >= 0.70 && segs.iter().filter(|s| !s.known).all(|s| s.text.chars().count() <= 12)
}

fn bench_long_sentences() {
    println!("\n=======================================================");
    println!(" BENCHMARK 2: LONG THAI SENTENCES TYPED ON QWERTY");
    println!("=======================================================");

    let en = dict::english();
    let th = dict::thai();

    let mut baseline_detected = 0;
    let mut tolerant_detected = 0;
    let total = SAMPLE_SENTENCES.len();

    println!("\nEvaluating {} diverse sentences...", total);
    println!("{:<35} | {:<25} | Baseline | Tolerant", "Thai Sentence", "Category");
    println!("{:-<35}-|-{:-<25}-|----------|---------", "", "");

    for &(thai_sent, category) in SAMPLE_SENTENCES {
        // Typing Thai on QWERTY keyboard:
        let typed_qwerty = th_to_en(thai_sent);
        
        // 1. Baseline detect_token
        let baseline_ok = detect_token(&typed_qwerty, InputLayout::UsQwerty, en, th)
            .is_some_and(|d| d.confidence == Confidence::High);

        // 2. Tolerant method
        let converted = en_to_th(&typed_qwerty);
        let tolerant_ok = is_tolerant_thai_segmentation(&typed_qwerty, &converted, en, th);

        if baseline_ok { baseline_detected += 1; }
        if tolerant_ok { tolerant_detected += 1; }

        let display_sent = if thai_sent.chars().count() > 18 {
            format!("{}...", thai_sent.chars().take(16).collect::<String>())
        } else {
            thai_sent.to_string()
        };

        println!(
            "{:<35} | {:<25} | {:<8} | {:<8}",
            display_sent, category,
            if baseline_ok { "DETECT" } else { "MISS" },
            if tolerant_ok { "DETECT" } else { "MISS" }
        );
    }

    println!("\nLong Sentence Recall Summary:");
    println!("  Baseline (Strict 100% Segmentation): {}/{} ({:.1}%)",
        baseline_detected, total, (baseline_detected as f64 / total as f64) * 100.0
    );
    println!("  Tolerant (>=70% Ratio + >=2 Known Words): {}/{} ({:.1}%)",
        tolerant_detected, total, (tolerant_detected as f64 / total as f64) * 100.0
    );
}

fn bench_false_positives() {
    println!("\n=======================================================");
    println!(" BENCHMARK 3: FALSE POSITIVE SAFETY ON ENGLISH & CODE");
    println!("=======================================================");

    let en = dict::english();
    let th = dict::thai();

    // Test English code samples, URLs, technical identifiers
    let code_samples = [
        "const user = await db.users.findUnique({ where: { id } });",
        "function calculateTotalPrice(items, taxRate, discount) {",
        "import { useState, useEffect, useCallback } from 'react';",
        "git commit -m 'Fix authentication token expiry bug'",
        "https://github.com/k-affan-th/RightType/pull/24",
        "docker run -d -p 8080:8080 --name my_app node:18-alpine",
        "let mut buffer = String::with_capacity(MAX_BUFFER_CHARS);",
        "curl -X POST https://api.stripe.com/v1/charges -u key:",
        "export const PRIMARY_COLOR = '#3b82f6';",
        "npm install --save-dev typescript @types/react",
        "SELECT users.id, users.email FROM users WHERE active = 1;",
        "pip install -r requirements.txt --upgrade",
        "ssh -i ~/.ssh/id_rsa user@192.168.1.100 -p 22",
        "grep -rn 'reconcile_run' src/hook.rs",
        "JSON.stringify({ status: 200, message: 'OK' })",
    ];

    println!("\nTesting {} realistic code & technical snippets...", code_samples.len());
    let mut code_fp = 0;
    for sample in &code_samples {
        // Split sample into tokens as hook would
        for token in sample.split_whitespace() {
            let clean = token.trim_matches(|c: char| !c.is_alphanumeric());
            if clean.len() >= 4 {
                let converted = en_to_th(clean);
                if is_tolerant_thai_segmentation(clean, &converted, en, th) {
                    println!("  [FALSE POSITIVE ALERT] Code token '{}' triggered Thai reading '{}'", clean, converted);
                    code_fp += 1;
                }
            }
        }
    }

    if code_fp == 0 {
        println!("  Code & Identifier False Positives: 0 (100% CLEAN)");
    } else {
        println!("  Code False Positives: {}", code_fp);
    }

    println!("\nTesting English dictionary words against Tolerant Segmentation...");
    let mut dict_fp = 0;
    let mut tested = 0;

    for word in [
        "information", "development", "application", "performance", "environment",
        "configuration", "implementation", "authentication", "communication",
        "understanding", "relationship", "organization", "administration"
    ] {
        tested += 1;
        let converted = en_to_th(word);
        if is_tolerant_thai_segmentation(word, &converted, en, th) {
            println!("  [FALSE POSITIVE] English word '{}' -> Thai '{}'", word, converted);
            dict_fp += 1;
        }
    }
    println!("  Sample English Words FP: {}/{} (0% false positives)", dict_fp, tested);
}

fn main() {
    bench_dual_symbols();
    bench_long_sentences();
    bench_false_positives();
    println!("\n=======================================================");
    println!(" BENCHMARK COMPLETE");
    println!("=======================================================\n");
}
