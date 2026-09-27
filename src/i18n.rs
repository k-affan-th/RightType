//! User-interface strings in English and Thai.
//!
//! Every visible string goes through [`tr`], so a missing translation is a
//! compile error rather than an English word left in a Thai window. The
//! language is process-wide: the Windows layer picks it from the config or the
//! Windows display language at startup, and windows opened afterwards use it.

use std::sync::atomic::{AtomicU8, Ordering};

/// A supported interface language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    En = 0,
    Th = 1,
}

impl Lang {
    /// The config value for this language.
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Th => "th",
        }
    }

    /// Parse a config value; anything else means "follow Windows".
    pub fn from_code(code: &str) -> Option<Lang> {
        match code.trim().to_ascii_lowercase().as_str() {
            "en" => Some(Lang::En),
            "th" => Some(Lang::Th),
            _ => None,
        }
    }

    /// Thai when the Windows display language's primary language is Thai.
    pub fn from_windows_langid(langid: u16) -> Lang {
        const LANG_THAI: u16 = 0x1E;
        if langid & 0x3FF == LANG_THAI {
            Lang::Th
        } else {
            Lang::En
        }
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(Lang::En as u8);

/// The interface language in use.
pub fn lang() -> Lang {
    if CURRENT.load(Ordering::Relaxed) == Lang::Th as u8 {
        Lang::Th
    } else {
        Lang::En
    }
}

/// Switch the interface language for everything drawn from now on.
pub fn set_lang(lang: Lang) {
    CURRENT.store(lang as u8, Ordering::Relaxed);
}

macro_rules! texts {
    ($($key:ident => $en:expr, $th:expr;)*) => {
        /// A translatable string.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum T { $($key),* }

        impl T {
            /// Every key, for tests.
            pub const ALL: &'static [T] = &[$(T::$key),*];

            /// This string in `lang`.
            pub fn get(self, lang: Lang) -> &'static str {
                match (self, lang) {
                    $((T::$key, Lang::En) => $en, (T::$key, Lang::Th) => $th,)*
                }
            }
        }
    };
}

texts! {
    // Tray menu. `&&` shows one ampersand in a menu item.
    TrayEnabled => "Enabled", "เปิดใช้งาน";
    TrayAuto => "Auto mode", "โหมดอัตโนมัติ";
    TrayManual => "Manual mode", "โหมดกดแก้เอง";
    TraySuggest => "Suggest mode", "โหมดแนะนำ";
    TrayLearn => "Learn new words", "เรียนรู้คำใหม่";
    TrayStartup => "Start with Windows", "เปิดพร้อม Windows";
    TraySettings => "Settings…", "ตั้งค่า…";
    TrayStats => "Statistics…", "สถิติ…";
    TrayHelp => "Hotkeys && help…", "ปุ่มลัดและวิธีใช้…";
    TrayQuit => "Quit", "ออกจากโปรแกรม";

    // Modes and state, as short labels.
    ModeAuto => "Auto", "อัตโนมัติ";
    ModeSuggest => "Suggest", "แนะนำ";
    ModeManual => "Manual", "กดแก้เอง";
    StateOff => "Off", "ปิดอยู่";
    StateHookLost => "Not working — retrying", "หยุดทำงาน — กำลังลองใหม่";

    // Toasts.
    ToastOn => "RightType is on", "RightType เปิดแล้ว";
    ToastOff => "RightType is off", "RightType ปิดแล้ว";
    ToastModeAuto => "Auto mode", "โหมดอัตโนมัติ";
    ToastModeSuggest => "Suggest mode", "โหมดแนะนำ";
    ToastModeManual => "Manual mode", "โหมดกดแก้เอง";
    ToastLearnOn => "Learning new words", "เปิดการเรียนรู้คำใหม่";
    ToastLearnOff => "Not learning new words", "ปิดการเรียนรู้คำใหม่";
    ToastUndo => "Undone", "ย้อนกลับแล้ว";
    ToastHookLost => "RightType lost the keyboard — retrying…", "RightType ตรวจจับคีย์บอร์ดไม่ได้ — กำลังลองใหม่…";
    ToastHookBack => "RightType is working again", "RightType กลับมาทำงานแล้ว";
    ToastSaved => "Saved", "บันทึกแล้ว";
    ToastLearnedCleared => "Learned words cleared", "ล้างคำที่เรียนรู้แล้ว";
    ToastSuggestAccept => "Alt+CapsLock", "Alt+CapsLock";
    ErrUndoInject => "RightType: could not undo", "RightType: ย้อนกลับไม่สำเร็จ";
    ErrSuggestInject => "RightType: could not apply the suggestion", "RightType: ใช้คำแนะนำไม่สำเร็จ";
    ErrCorrectionInject => "RightType: could not fix the word", "RightType: แก้คำไม่สำเร็จ";
    ErrClipboardBusy => "RightType: the clipboard is busy — try again", "RightType: คลิปบอร์ดกำลังถูกใช้ ลองอีกครั้ง";
    ErrClipboardNotPlain => "RightType: converting a selection needs a plain-text clipboard", "RightType: การแปลงข้อความที่เลือกต้องใช้คลิปบอร์ดที่เป็นข้อความธรรมดา";
    ErrModifiers => "RightType: could not release held keys", "RightType: ปล่อยปุ่มที่กดค้างไม่สำเร็จ";
    ErrCopy => "RightType: could not copy the selection", "RightType: คัดลอกข้อความที่เลือกไม่สำเร็จ";
    ErrNothingCopied => "RightType: nothing was selected", "RightType: ไม่ได้เลือกข้อความ";
    ErrNotUnicode => "RightType: the selection is not text", "RightType: สิ่งที่เลือกไม่ใช่ข้อความ";
    ErrRestoreClipboard => "RightType: could not restore the clipboard", "RightType: คืนค่าคลิปบอร์ดไม่สำเร็จ";
    ErrInjectConversion => "RightType: could not type the conversion", "RightType: พิมพ์ข้อความที่แปลงไม่สำเร็จ";
    ErrSelectionUndo => "RightType: could not undo the selection", "RightType: ย้อนกลับข้อความที่เลือกไม่สำเร็จ";

    // Settings window.
    SettingsTitle => "RightType — Settings", "RightType — ตั้งค่า";
    NavGeneral => "General", "ทั่วไป";
    NavHotkeys => "Hotkeys", "ปุ่มลัด";
    NavLearned => "Learned words", "คำที่เรียนรู้";
    NavBlocked => "Blocked apps", "แอปที่ไม่ทำงาน";
    NavAbout => "Privacy & about", "ความเป็นส่วนตัว";
    HeadMode => "Correction mode", "วิธีแก้คำ";
    DescAuto => "Fixes words as you type — Thai as soon as your keys spell a Thai word, English when you press Space.", "แก้ให้ทันทีขณะพิมพ์ — เป็นภาษาไทยทันทีที่ปุ่มที่กดสะกดเป็นคำไทย และเป็นภาษาอังกฤษเมื่อกดเว้นวรรค";
    DescSuggest => "Shows the fix in a small hint. Press Alt+CapsLock to use it.", "แสดงคำที่ถูกเป็นคำแนะนำเล็ก ๆ กด Alt+CapsLock เพื่อใช้คำนั้น";
    DescManual => "Never changes text by itself. Press Shift+Backspace to fix the last word.", "ไม่แก้ข้อความเอง กด Shift+Backspace เพื่อแก้คำล่าสุด";
    HeadBehaviour => "Behaviour", "การทำงาน";
    RowEnabled => "RightType is on", "เปิดใช้งาน RightType";
    SubEnabled => "Turn it off or on anytime with Ctrl+Alt+CapsLock.", "เปิดหรือปิดได้ทุกเมื่อด้วย Ctrl+Alt+CapsLock";
    RowStartup => "Start with Windows", "เปิดพร้อม Windows";
    SubStartup => "Runs quietly in the notification area when you sign in.", "ทำงานเงียบ ๆ ที่มุมจอเมื่อเข้าสู่ระบบ";
    RowLearn => "Learn new words", "เรียนรู้คำใหม่";
    SubLearn => "Remembers words you type often or put back. Saved on this PC only.", "จำคำที่คุณพิมพ์บ่อยหรือแก้กลับ บันทึกไว้ในเครื่องนี้เท่านั้น";
    LearnedCount => "Learned words: {n}", "คำที่เรียนรู้แล้ว: {n}";
    BtnEditLearned => "Edit…", "แก้ไข…";
    HeadLanguage => "Language", "ภาษา";
    HeadHotkeys => "Hotkeys", "ปุ่มลัด";
    NoteHotkeysFixed => "Hotkeys work in every app and are fixed in this version.", "ปุ่มลัดใช้ได้ทุกแอป และยังเปลี่ยนไม่ได้ในเวอร์ชันนี้";
    HkFlip => "Fix or flip back the last word", "แก้หรือสลับคำล่าสุดกลับ";
    HkSelection => "Convert the selected text", "แปลงข้อความที่เลือก";
    HkCycle => "Switch correction mode", "สลับโหมดการแก้คำ";
    HkUndo => "Undo the last fix", "ย้อนการแก้ล่าสุด";
    HkAccept => "Use the suggestion", "ใช้คำแนะนำ";
    HkPanic => "Turn RightType off / on", "ปิด / เปิด RightType";
    LearnedIntro => "RightType treats these as real words and never converts them. One word per line, Thai or English — delete a line to forget that word.", "RightType ถือว่าคำเหล่านี้เป็นคำจริงและจะไม่แปลงเลย หนึ่งคำต่อบรรทัด ภาษาไทยหรืออังกฤษก็ได้ — ลบบรรทัดออกเพื่อให้ลืมคำนั้น";
    BtnSaveLearned => "Save", "บันทึก";
    BtnClearAll => "Clear all", "ล้างทั้งหมด";
    LearnedSaved => "Saved {n} words.", "บันทึก {n} คำแล้ว";
    LearnedSkipped => "Saved {n} words. Skipped {k} lines that are not a single word.", "บันทึก {n} คำแล้ว ข้าม {k} บรรทัดที่ไม่ใช่คำเดียว";
    HeadBlocked => "Blocked apps", "แอปที่ไม่ทำงาน";
    BlockedAlways => "RightType always stays out of password fields, terminals, password managers and crypto wallets.", "RightType ไม่ทำงานในช่องรหัสผ่าน เทอร์มินัล โปรแกรมจัดการรหัสผ่าน และกระเป๋าคริปโตเสมอ";
    BlockedAdd => "Also stay out of these apps — one program name per line, for example notepad.exe:", "ไม่ทำงานในแอปเหล่านี้ด้วย — หนึ่งชื่อโปรแกรมต่อบรรทัด เช่น notepad.exe:";
    BtnSaveList => "Save list", "บันทึกรายการ";
    HeadPrivacy => "Privacy", "ความเป็นส่วนตัว";
    Privacy1 => "Nothing you type is written to disk.", "สิ่งที่คุณพิมพ์ไม่ถูกบันทึกลงดิสก์";
    Privacy2 => "No internet access — RightType never connects to a network.", "ไม่ใช้อินเทอร์เน็ต — RightType ไม่เชื่อมต่อเครือข่ายเลย";
    Privacy3 => "Seed phrases, private keys and passwords are recognised and left alone.", "จดจำ seed phrase คีย์ส่วนตัว และรหัสผ่านได้ และจะไม่แตะต้อง";
    Privacy4 => "The word being typed is kept in memory only, and wiped at every space.", "คำที่กำลังพิมพ์อยู่ในหน่วยความจำเท่านั้น และถูกล้างทุกครั้งที่เว้นวรรค";
    HeadAbout => "About", "เกี่ยวกับ";
    AboutVersion => "RightType {v}", "RightType {v}";
    AboutLicense => "Free and open source — MIT or Apache-2.0.", "ฟรีและโอเพนซอร์ส — MIT หรือ Apache-2.0";
    BtnClose => "Close", "ปิด";

    // Welcome / help window.
    WelcomeTitle => "Welcome to RightType", "ยินดีต้อนรับสู่ RightType";
    HelpTitle => "RightType — Hotkeys", "RightType — ปุ่มลัด";
    WelcomeHeadline => "Wrong layout? Fixed — no retyping.", "พิมพ์ผิดภาษา? แก้ให้ทันที ไม่ต้องพิมพ์ใหม่";
    WelcomeSub => "Thai Kedmanee ↔ US English, in every app. Password fields, terminals and wallets are always left alone.", "ไทยเกษมณี ↔ อังกฤษ US ใช้ได้ทุกแอป และไม่ยุ่งกับช่องรหัสผ่าน เทอร์มินัล และกระเป๋าคริปโต";
    WelcomeExample => "l;ylfu  →  สวัสดี", "l;ylfu  →  สวัสดี";
    HelpHeadline => "Hotkeys", "ปุ่มลัด";
    WelcomeMode => "Current mode: {mode}. Switch anytime with Ctrl+CapsLock.", "โหมดตอนนี้: {mode} สลับได้ทุกเมื่อด้วย Ctrl+CapsLock";
    BtnGetStarted => "Get started", "เริ่มใช้งาน";

    // Statistics window.
    StatsTitle => "RightType — Statistics", "RightType — สถิติ";
    StatsHead => "Since RightType started", "ตั้งแต่เปิดโปรแกรม";
    StatsAuto => "Fixed automatically", "แก้อัตโนมัติ";
    StatsManual => "Fixed with a hotkey", "แก้ด้วยปุ่มลัด";
    StatsLearned => "Learned words", "คำที่เรียนรู้";
    StatsNote => "Counts start again when RightType restarts. Nothing you type is saved.", "ตัวเลขจะเริ่มใหม่เมื่อเปิดโปรแกรมใหม่ และไม่มีการบันทึกสิ่งที่คุณพิมพ์";
}

/// `key` in the current interface language.
pub fn tr(key: T) -> &'static str {
    key.get(lang())
}

/// `key` with `{name}` placeholders filled in.
pub fn trf(key: T, args: &[(&str, &str)]) -> String {
    let mut text = tr(key).to_string();
    for (name, value) in args {
        text = text.replace(&format!("{{{name}}}"), value);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_thai(c: char) -> bool {
        ('\u{0E00}'..='\u{0E7F}').contains(&c)
    }

    #[test]
    fn every_string_exists_in_both_languages() {
        for key in T::ALL {
            assert!(!key.get(Lang::En).is_empty(), "{key:?} en");
            assert!(!key.get(Lang::Th).is_empty(), "{key:?} th");
        }
    }

    #[test]
    fn thai_strings_are_actually_thai() {
        // Strings that are only key names, a product name or an example may
        // stay identical; everything else must contain Thai.
        const SAME: &[T] = &[T::ToastSuggestAccept, T::AboutVersion, T::WelcomeExample];
        for key in T::ALL {
            if SAME.contains(key) {
                continue;
            }
            assert!(
                key.get(Lang::Th).chars().any(is_thai),
                "{key:?} has no Thai text"
            );
        }
    }

    #[test]
    fn placeholders_match_between_languages() {
        for key in T::ALL {
            for name in ["{n}", "{k}", "{v}", "{mode}"] {
                assert_eq!(
                    key.get(Lang::En).contains(name),
                    key.get(Lang::Th).contains(name),
                    "{key:?} {name}"
                );
            }
        }
    }

    #[test]
    fn language_codes_round_trip_and_windows_thai_is_detected() {
        for lang in [Lang::En, Lang::Th] {
            assert_eq!(Lang::from_code(lang.code()), Some(lang));
        }
        assert_eq!(Lang::from_code("auto"), None);
        assert_eq!(Lang::from_windows_langid(0x041E), Lang::Th);
        assert_eq!(Lang::from_windows_langid(0x0409), Lang::En);
    }

    #[test]
    fn placeholders_are_filled() {
        set_lang(Lang::En);
        assert_eq!(trf(T::LearnedCount, &[("n", "12")]), "Learned words: 12");
    }
}
