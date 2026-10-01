//! Common Thai misspellings, put right (opt-in).
//!
//! A short, hand-checked list of misspellings people make all the time
//! (`อนุญาติ` for `อนุญาต`, `โอกาศ` for `โอกาส`). Only these: RightType is
//! not a spelling checker, and a word it does not know is left alone.
//!
//! A fix is made only when it makes the typed text read better: with the
//! misspelling put right, fewer of its letters are left over as unknown to
//! the Thai dictionary. So `กระทิ` inside `กระทิง` (a real word) is left
//! alone. A few misspellings that are always wrong even though their parts
//! are words (`นะค่ะ`) are fixed as long as the text does not read worse.
//!
//! The Windows layer shows each fix in its own colour with the way back
//! (Shift+Backspace), so a fix is never a surprise.

use crate::dict::Dictionary;
use crate::segment::segment;

/// `(misspelt, right, always)`.
pub const PAIRS: &[(&str, &str, bool)] = &[
    ("อนุญาติ", "อนุญาต", false),
    ("สังเกตุ", "สังเกต", false),
    ("ผลลัพท์", "ผลลัพธ์", false),
    ("ปรากฎ", "ปรากฏ", false),
    ("กฏหมาย", "กฎหมาย", false),
    ("กฏเกณฑ์", "กฎเกณฑ์", false),
    ("อุปสรรถ", "อุปสรรค", false),
    ("รสชาด", "รสชาติ", false),
    ("สำอางค์", "สำอาง", false),
    ("บุคคลากร", "บุคลากร", false),
    ("ปาฏิหารย์", "ปาฏิหาริย์", false),
    ("ภาพยนต์", "ภาพยนตร์", false),
    ("กระทิ", "กะทิ", false),
    ("กระเทย", "กะเทย", false),
    ("กระตือรือล้น", "กระตือรือร้น", false),
    ("สังสรร", "สังสรรค์", false),
    ("ผาสุข", "ผาสุก", false),
    ("จราจล", "จลาจล", false),
    ("อานิสงฆ์", "อานิสงส์", false),
    ("ซีรี่ย์", "ซีรีส์", false),
    ("ปลักหักพัง", "ปรักหักพัง", false),
    ("ลิปสติค", "ลิปสติก", false),
    ("นะค่ะ", "นะคะ", true),
    ("กระเพรา", "กะเพรา", false),
    ("บิณฑบาตร", "บิณฑบาต", false),
    ("อัฒจรรย์", "อัฒจันทร์", false),
    ("พรรณา", "พรรณนา", false),
    ("ไวยกรณ์", "ไวยากรณ์", false),
    ("ฉนั้น", "ฉะนั้น", false),
    ("บันทัดฐาน", "บรรทัดฐาน", false),
    ("บรรได", "บันได", false),
    ("บรรเทิง", "บันเทิง", false),
    ("บันเทา", "บรรเทา", false),
    ("ผลัดผ่อน", "ผัดผ่อน", false),
    ("เวทมนต์", "เวทมนตร์", false),
    ("ชลอ", "ชะลอ", false),
    ("นานับประการ", "นานัปการ", false),
    ("คอลัมภ์", "คอลัมน์", false),
    ("ออฟฟิต", "ออฟฟิศ", false),
    ("ซอฟแวร์", "ซอฟต์แวร์", false),
    ("คลิ๊ก", "คลิก", false),
    ("เปอร์เซ็นต", "เปอร์เซ็นต์", false),
    ("มาตราฐาน", "มาตรฐาน", false),
    ("เชี่ยวชาน", "เชี่ยวชาญ", false),
    ("ประสบการ", "ประสบการณ์", false),
    ("เหตุการ", "เหตุการณ์", false),
    ("สถานการ", "สถานการณ์", false),
    ("สัญลักษ", "สัญลักษณ์", false),
    ("โลกาภิวัฒน์", "โลกาภิวัตน์", false),
    ("ข้าวเหนียวมูล", "ข้าวเหนียวมูน", false),
    ("กงศุล", "กงสุล", false),
    ("ผลิตภัณ", "ผลิตภัณฑ์", false),
    ("อุทิส", "อุทิศ", false),
    ("สัปดาห", "สัปดาห์", false),
    ("คำนวน", "คำนวณ", false),
    ("ทรัพยากรณ์", "ทรัพยากร", false),
    ("อนุมัต", "อนุมัติ", false),
    ("ผู้เชี่ยวชาน", "ผู้เชี่ยวชาญ", false),
    ("ปรัชญ์", "ปรัชญา", false),
    ("แกงบวช", "แกงบวด", false),
    ("ปฎิบัติ", "ปฏิบัติ", false),
    ("ปฎิทิน", "ปฏิทิน", false),
    ("ปฎิเสธ", "ปฏิเสธ", false),
    ("เทคนิก", "เทคนิค", false),
    ("วิดิโอ", "วิดีโอ", false),
    ("ปรับปรุ่ง", "ปรับปรุง", false),
    ("ผู้บรืหาร", "ผู้บริหาร", false),
    ("กะเพาะ", "กระเพาะ", false),
    ("ประสบการณ", "ประสบการณ์", false),
];

/// Letters of `text` the dictionary does not account for.
fn unknown_letters(text: &str, th: &Dictionary) -> usize {
    segment(text, th)
        .iter()
        .filter(|s| !s.known)
        .map(|s| s.text.chars().count())
        .sum()
}

/// `run` (Thai, as typed) with its common misspellings put right, and the
/// first one fixed as `(misspelt, right)`. `None` when there is nothing to
/// fix, or fixing would not make it read better.
pub fn fix(run: &str, th: &Dictionary) -> Option<(String, &'static str, &'static str)> {
    let mut out = run.to_string();
    let mut first = None;
    let mut always_only = true;
    for &(wrong, right, always) in PAIRS {
        let mut from = 0;
        while let Some(at) = out[from..].find(wrong).map(|i| i + from) {
            // Part of the right spelling already (`เหตุการ` in `เหตุการณ์`).
            if right.starts_with(wrong) && out[at..].starts_with(right) {
                from = at + wrong.len();
                continue;
            }
            out.replace_range(at..at + wrong.len(), right);
            first.get_or_insert((wrong, right));
            always_only &= always;
            from = at + right.len();
        }
    }
    let first = first?;
    let before = unknown_letters(run, th);
    let after = unknown_letters(&out, th);
    let better = after < before || (always_only && after <= before);
    better.then_some((out, first.0, first.1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dict;

    #[test]
    fn every_right_spelling_is_a_word_and_no_misspelling_is() {
        let th = dict::thai();
        let mut bad = Vec::new();
        for &(wrong, right, always) in PAIRS {
            if !always && th.contains(wrong) {
                bad.push(format!("{wrong} is in the dictionary"));
            }
            if unknown_letters(right, th) != 0 {
                bad.push(format!("{right} is not known"));
            }
        }
        assert!(bad.is_empty(), "{bad:#?}");
    }

    #[test]
    fn misspellings_are_put_right_in_a_run() {
        let th = dict::thai();
        assert_eq!(
            fix("ขออนุญาติครับ", th).map(|f| f.0),
            Some("ขออนุญาตครับ".to_string())
        );
        assert_eq!(
            fix("ผลลัพท์ที่ได้", th).map(|f| f.0),
            Some("ผลลัพธ์ที่ได้".to_string())
        );
        assert_eq!(
            fix("ขอบคุณนะค่ะ", th).map(|f| f.0),
            Some("ขอบคุณนะคะ".to_string())
        );
    }

    #[test]
    fn no_dictionary_word_is_changed() {
        // Every bundled Thai word, alone: a real word is never "fixed".
        let th = dict::thai();
        let changed: Vec<String> = include_str!("../assets/th_words.txt")
            .lines()
            .filter(|w| fix(w, th).is_some())
            .map(str::to_string)
            .collect();
        assert!(changed.is_empty(), "{changed:?}");
    }

    #[test]
    fn real_words_that_contain_a_misspelling_are_left_alone() {
        let th = dict::thai();
        // กระทิ inside กระทิง (a bull).
        assert_eq!(fix("กระทิง", th), None);
        // Already right.
        assert_eq!(fix("เหตุการณ์", th), None);
        assert_eq!(fix("สวัสดี", th), None);
    }
}
