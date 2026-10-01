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
    TrayFix => "Fix text…", "ซ่อมข้อความ…";
    TraySettings => "Settings…", "ตั้งค่า…";
    TrayStats => "Statistics…", "สถิติ…";
    TrayHelp => "Hotkeys && help…", "ปุ่มลัดและวิธีใช้…";
    TrayReport => "Save a problem report…", "บันทึกรายงานปัญหา…";
    TrayQuit => "Quit", "ออกจากโปรแกรม";
    TrayPause => "Pause", "หยุดชั่วคราว";
    TrayPause10 => "For 10 minutes", "10 นาที";
    TrayPause30 => "For 30 minutes", "30 นาที";
    TrayPause60 => "For 1 hour", "1 ชั่วโมง";
    TrayResume => "Resume now", "กลับมาทำงานเดี๋ยวนี้";
    TrayThisApp => "In this app", "ในแอปนี้";
    TrayNoApp => "Type in an app first", "พิมพ์ในแอปที่ต้องการก่อน";
    TrayAppDefault => "Use the default mode", "ใช้โหมดปกติ";
    TrayAppOff => "Off in this app", "ปิดในแอปนี้";

    // Modes and state, as short labels.
    ModeAuto => "Auto", "อัตโนมัติ";
    ModeSuggest => "Suggest", "แนะนำ";
    ModeManual => "Manual", "กดแก้เอง";
    StateOff => "Off", "ปิดอยู่";
    StateHookLost => "Not working — retrying", "หยุดทำงาน — กำลังลองใหม่";
    StatePaused => "Paused — back in {n} min", "หยุดชั่วคราว — กลับมาในอีก {n} นาที";
    StateAppMode => "{mode} in {app}", "{mode} ใน {app}";
    ModeOff => "Off", "ปิด";

    // Toasts.
    ToastOn => "RightType is on", "RightType เปิดแล้ว";
    ToastOff => "RightType is off", "RightType ปิดแล้ว";
    ToastModeAuto => "Auto mode", "โหมดอัตโนมัติ";
    ToastModeSuggest => "Suggest mode", "โหมดแนะนำ";
    ToastModeManual => "Manual mode", "โหมดกดแก้เอง";
    ToastLearnOn => "Learning new words", "เปิดการเรียนรู้คำใหม่";
    ToastLearnOff => "Not learning new words", "ปิดการเรียนรู้คำใหม่";
    ToastUndo => "Undone", "ย้อนกลับแล้ว";
    ToastFlippedWords => "Flipped back {n} words", "แก้ย้อน {n} คำแล้ว";
    ToastFlippedOne => "Flipped 1 word", "แก้ 1 คำแล้ว";
    ToastFlippedOneBack => "Flipped 1 word · again to put it back", "แก้ 1 คำแล้ว · กดอีกครั้งเพื่อคืนค่าเดิม";
    ToastFlippedOneMore => "Flipped 1 word · again for the one before", "แก้ 1 คำแล้ว · กดอีกครั้งแก้คำก่อนหน้า";
    ToastNothingToFlip => "Nothing to flip here (the cursor moved, or no more words)", "ไม่มีคำให้แก้ตรงนี้ (เคอร์เซอร์ย้ายแล้ว หรือไม่มีคำก่อนหน้า)";
    ToastPaused => "Paused for {n} minutes", "หยุดชั่วคราว {n} นาที";
    ToastAppMode => "{mode} in {app}", "{mode} ใน {app}";
    ToastHookLost => "RightType lost the keyboard — retrying…", "RightType ตรวจจับคีย์บอร์ดไม่ได้ — กำลังลองใหม่…";
    ToastHookBack => "RightType is working again", "RightType กลับมาทำงานแล้ว";
    ToastSaved => "Saved", "บันทึกแล้ว";
    PaletteUpper => "UPPER CASE (selection)", "ตัวพิมพ์ใหญ่ทั้งหมด (ข้อความที่เลือก)";
    PaletteLower => "lower case (selection)", "ตัวพิมพ์เล็กทั้งหมด (ข้อความที่เลือก)";
    PaletteTitle => "Title Case (selection)", "ขึ้นต้นคำด้วยตัวใหญ่ (ข้อความที่เลือก)";
    PaletteSwapCase => "sWAP cASE — undo CapsLock (selection)", "สลับตัวเล็ก/ใหญ่ — แก้ CapsLock ค้าง (ข้อความที่เลือก)";
    PaletteSwapDigits => "Thai digits ↔ 0–9 in the selection", "เลขไทย ↔ เลขอารบิก ในข้อความที่เลือก";
    PaletteTrayLanguage => "Tray icon shows TH / EN", "ไอคอนถาดระบบแสดง TH / EN";
    PaletteCapsSwitch => "CapsLock switches Thai/English", "CapsLock สลับภาษาไทย/อังกฤษ";
    ToastCapsSwitchOn => "Tap CapsLock to switch language · hold it for CAPS", "แตะ CapsLock เพื่อสลับภาษา · กดค้างเพื่อพิมพ์ตัวใหญ่";
    ToastCapsSwitchOff => "CapsLock is CapsLock again", "CapsLock กลับมาเป็นปุ่มตัวพิมพ์ใหญ่ตามเดิม";
    TipShiftBackspace => "↶ Shift+Backspace", "↶ Shift+Backspace ย้อนได้";
    PaletteFixField => "Fix this field", "ซ่อมทั้งช่องนี้";
    ErrFieldTooLong => "This field is too long to fix here — use Fix text", "ช่องนี้ยาวเกินไป ใช้หน้าต่างซ่อมข้อความแทน";
    ToastNothingToFix => "Nothing to fix in this field", "ไม่มีคำที่ต้องแก้ในช่องนี้";
    ToastFixedWords => "Fixed {n} words · Ctrl+Z undoes it", "แก้ {n} คำ · Ctrl+Z ย้อนได้";
    PaletteKeepAsTyped => "Never convert “{word}”", "ไม่ต้องแปลง “{word}” อีก";
    ToastCapsOff => "CapsLock was on — word fixed, CapsLock off · Shift+Backspace if you meant capitals", "CapsLock ค้าง — แก้คำและปิด CapsLock แล้ว · ตั้งใจพิมพ์ตัวใหญ่? กด Shift+Backspace";
    ToastCapsKept => "Capitals kept, CapsLock on again", "คืนตัวพิมพ์ใหญ่และเปิด CapsLock ให้แล้ว";
    ToastModeCode => "Code mode: only what is clearly a slip — never names, Thai only in comments and strings", "โหมดโค้ด: แก้เฉพาะที่พิมพ์ผิดแป้นชัด ๆ ไม่แตะชื่อตัวแปร และแปลงเป็นไทยเฉพาะใน comment/string";
    RowSyncSettings => "Sync settings and snippets too", "ซิงก์การตั้งค่าและคำย่อด้วย";
    SubSyncSettings => "Modes, apps, hotkeys and snippets, in the same folder, for every PC that uses it. Off by default.", "โหมด รายการแอป ปุ่มลัด และคำย่อ เก็บในโฟลเดอร์เดียวกัน ใช้ร่วมกันทุกเครื่องที่ใช้โฟลเดอร์นี้ (ปิดไว้เป็นค่าเริ่มต้น)";
    SyncNeedsFolder => "Choose a sync folder first.", "เลือกโฟลเดอร์สำหรับซิงก์ก่อน";
    NavSnippets => "Snippets", "คำย่อ";
    SnippetsIntro => "Type a short trigger, then Space or Enter, and it becomes the text. Shift+Backspace right after puts the trigger back.", "พิมพ์คำย่อแล้วกด Space หรือ Enter จะกลายเป็นข้อความเต็ม กด Shift+Backspace ทันทีเพื่อคืนคำย่อ";
    ColTrigger => "Trigger", "คำย่อ";
    ColText => "Text", "ข้อความ";
    ColKeyboard => "Keyboard", "แป้นพิมพ์";
    ScopeThai => "Thai only", "แป้นไทย";
    ScopeEnglish => "English only", "แป้นอังกฤษ";
    ScopeEither => "Either", "ทั้งสองแป้น";
    BtnSaveSnippet => "Add / save", "เพิ่ม / บันทึก";
    SnippetsNote => "“Either” matches the keys you press, so ;addr works on the Thai keyboard too. Start triggers with ; so they never clash with a word. Not in password fields. Snippets are saved with the settings.", "“ทั้งสองแป้น” จับจากปุ่มที่กด ;addr จึงใช้ได้แม้เปิดแป้นไทยอยู่ แนะนำให้ขึ้นต้นด้วย ; จะได้ไม่ชนกับคำปกติ ไม่ทำงานในช่องรหัสผ่าน คำย่อถูกบันทึกไว้กับการตั้งค่า";
    SnipTriggerLength => "A trigger is 2 to 32 characters.", "คำย่อต้องยาว 2–32 ตัวอักษร";
    SnipTriggerSpace => "A trigger has no spaces.", "คำย่อต้องไม่มีช่องว่าง";
    SnipTextEmpty => "Write the text it becomes.", "ใส่ข้อความที่ต้องการ";
    SnipTextLong => "The text is too long (1,000 characters at most).", "ข้อความยาวเกินไป (ไม่เกิน 1,000 ตัวอักษร)";
    SnipTooMany => "200 snippets at most.", "มีคำย่อได้ไม่เกิน 200 รายการ";
    SnipRemoved => "Removed.", "ลบแล้ว";
    AppsIntro => "Apps with a mode of their own. Select one and pick a mode below, or right-click it. Apps not listed use the general mode.", "แอปที่มีโหมดของตัวเอง เลือกแถวแล้วกดโหมดด้านล่าง หรือคลิกขวาที่แถว แอปที่ไม่อยู่ในรายการใช้โหมดหลัก";
    ColApp => "App", "แอป";
    ColMode => "Mode", "โหมด";
    ColSetBy => "Set by", "ตั้งโดย";
    ColWhere => "Location", "ตำแหน่งไฟล์";
    SetByForNow => "This time only", "เฉพาะครั้งนี้";
    SetByYou => "You", "คุณ";
    SetByYouBlocked => "You (blocked)", "คุณ (บล็อก)";
    SetByDefault => "Default: code editor", "ค่าเริ่มต้น: โปรแกรมเขียนโค้ด";
    SetBySafety => "Safety (built in)", "ความปลอดภัย (ในตัว)";
    ModeAlwaysOff => "Always off 🔒", "ปิดเสมอ 🔒";
    ModeBlocked => "Off (blocked)", "ปิด (บล็อก)";
    BtnAddApp => "+ Add an app that is open…", "+ เพิ่มจากแอปที่เปิดอยู่…";
    BtnKeepMode => "Keep for good", "ใช้ตลอด";
    BtnRemoveApp => "Remove", "ลบออก";
    BtnShowFile => "Show the program file", "เปิดตำแหน่งไฟล์";
    AppsPickFirst => "Select an app in the list first.", "เลือกแอปในรายการก่อน";
    AppsBuiltIn => "Built in — RightType keeps this one as it is.", "ตั้งไว้ในตัว เปลี่ยนไม่ได้";
    AppsRemoved => "{app} uses the general mode again.", "{app} กลับไปใช้โหมดหลักแล้ว";
    AppsKeepWhat => "Only a mode chosen “this time only” can be kept.", "ใช้ได้กับแอปที่ตั้งไว้ “เฉพาะครั้งนี้” เท่านั้น";
    AppsNoneRunning => "No other app with a window is open.", "ไม่มีแอปอื่นที่เปิดหน้าต่างอยู่";
    AppsAdded => "Added {app} — now pick its mode below.", "เพิ่ม {app} แล้ว เลือกโหมดด้านล่างได้เลย";
    RowRestart => "Start again after a crash", "เปิดใหม่เองเมื่อโปรแกรมล่ม";
    SubRestart => "Not after you quit it or end it in Task Manager. From the next start.", "ไม่เปิดเองหลังจากคุณสั่งปิด หรือปิดจาก Task Manager มีผลตั้งแต่เปิดครั้งถัดไป";
    ToastOfferMode => "Fixes keep being taken back in {app} — {keys} to switch it to {mode}", "ใน {app} มีการย้อนคำที่แก้บ่อย กด {keys} เพื่อเปลี่ยนเป็นโหมด{mode}";
    PaletteOfferForNow => "{mode} in {app}, only this time", "ใช้โหมด{mode}ใน {app} เฉพาะครั้งนี้";
    PaletteOfferKeep => "{mode} in {app} from now on (add to the Apps list)", "ใช้โหมด{mode}ใน {app} ตลอด (เพิ่มในรายการแอป)";
    ToastModeForNow => "{mode} in {app} until RightType restarts", "ใช้โหมด{mode}ใน {app} จนกว่าจะปิด RightType";
    TrayCode => "Code (for code editors)", "โค้ด (สำหรับเขียนโปรแกรม)";
    ModeCode => "Code", "โค้ด";
    SayFixed => "Fixed: {word}", "แก้เป็น {word}";
    PaletteHistoryHint => "Recent words · Space ticks more than one · Enter flips them", "คำล่าสุด · Space เลือกหลายคำ · Enter แปลงคำที่เลือก";
    ToastSpellingFixed => "✎ Spelling: {wrong} → {right} · Backspace or Shift+Backspace puts it back", "✎ แก้คำสะกด: {wrong} → {right} · กด Backspace หรือ Shift+Backspace เพื่อคืนคำเดิม";
    ToastSpellingKept => "Put back as typed — “{word}” will not be changed again", "คืนคำที่พิมพ์ไว้แล้ว จะไม่แก้ “{word}” อีก";
    PaletteSpelling => "Fix common Thai misspellings", "แก้คำไทยที่สะกดผิดบ่อย";
    RowSpelling => "Fix common Thai misspellings", "แก้คำไทยที่สะกดผิดบ่อย";
    SubSpelling => "อนุญาติ → อนุญาต and about 60 more, in Auto. Each fix is shown; Backspace right after puts it back.", "เช่น อนุญาติ → อนุญาต และอีกราว 60 คำ (โหมดอัตโนมัติ) แจ้งทุกครั้งที่แก้ กด Backspace ทันทีเพื่อคืนคำเดิม";
    PaletteFieldOff => "Off in this field", "ปิดเฉพาะช่องนี้";
    PaletteFieldOn => "On again in this field", "เปิดในช่องนี้อีกครั้ง";
    ToastFieldOff => "Off in this field until RightType restarts — the rest of the app still works", "ปิดในช่องนี้แล้ว (จนกว่าจะปิด RightType) ช่องอื่นในแอปยังทำงานตามปกติ";
    ToastFieldOn => "On again in this field", "เปิดในช่องนี้แล้ว";
    ToastAlreadyRunning => "RightType is already running — it's in the tray (bottom right)", "RightType เปิดอยู่แล้ว อยู่ที่ tray มุมขวาล่าง";
    ToastReplaced => "Now running RightType {v} (closed {old})", "เปลี่ยนเป็น RightType {v} แล้ว (ปิดตัว {old})";
    ToastReplacedUnknown => "Now running RightType {v} (closed the copy that was running)", "เปลี่ยนเป็น RightType {v} แล้ว (ปิดตัวที่เปิดอยู่ก่อน)";
    ToastRestarted => "RightType stopped unexpectedly and was started again", "RightType หยุดทำงานกะทันหัน จึงเปิดใหม่ให้แล้ว";
    ToastVerifyDiffers => "This app showed the fix differently — check the last word (Ctrl+Z undoes it). RightType will type slower here.", "แอปนี้แสดงคำที่แก้ไม่ตรง ลองดูคำล่าสุด (Ctrl+Z ย้อนได้) ต่อไป RightType จะพิมพ์ช้าลงในแอปนี้";
    ToastLearnedCleared => "Learned words cleared", "ล้างคำที่เรียนรู้แล้ว";
    ToastSuggestAccept => "Alt+CapsLock", "Alt+CapsLock";
    ErrUndoInject => "RightType: could not undo", "RightType: ย้อนกลับไม่สำเร็จ";
    ErrSuggestInject => "RightType: could not apply the suggestion", "RightType: ใช้คำแนะนำไม่สำเร็จ";
    ErrCorrectionInject => "RightType: could not fix the word", "RightType: แก้คำไม่สำเร็จ";
    ErrClipboardBusy => "RightType: the clipboard is busy — try again", "RightType: คลิปบอร์ดกำลังถูกใช้ ลองอีกครั้ง";
    ErrSelectionNotShared => "RightType: this app does not share the selected text — use Fix text… in the tray", "RightType: แอปนี้ไม่ส่งข้อความที่เลือกให้ ใช้ ซ่อมข้อความ… ที่ถาดไอคอนแทน";
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
    NavBlocked => "Apps", "แอป";
    NavAbout => "Privacy & about", "ความเป็นส่วนตัว";
    HeadMode => "Correction mode", "วิธีแก้คำ";
    DescAuto => "Fixes words as you type — Thai as soon as your keys spell a Thai word, English when you press Space.", "แก้ให้ทันทีขณะพิมพ์ — เป็นภาษาไทยทันทีที่ปุ่มที่กดสะกดเป็นคำไทย และเป็นภาษาอังกฤษเมื่อกดเว้นวรรค";
    DescSuggest => "Shows the fix in a small hint next to the cursor. Press Tab (or Alt+CapsLock) to use it.", "แสดงคำที่ถูกเป็นคำแนะนำเล็ก ๆ ข้างเคอร์เซอร์ กด Tab (หรือ Alt+CapsLock) เพื่อใช้คำนั้น";
    DescManual => "Never changes text by itself. Press Shift+Backspace to fix the last word.", "ไม่แก้ข้อความเอง กด Shift+Backspace เพื่อแก้คำล่าสุด";
    HeadBehaviour => "Behaviour", "การทำงาน";
    RowEnabled => "RightType is on", "เปิดใช้งาน RightType";
    SubEnabled => "Turn it off or on anytime with Ctrl+Alt+CapsLock.", "เปิดหรือปิดได้ทุกเมื่อด้วย Ctrl+Alt+CapsLock";
    RowStartup => "Start with Windows", "เปิดพร้อม Windows";
    SubStartup => "Runs quietly in the notification area when you sign in.", "ทำงานเงียบ ๆ ที่มุมจอเมื่อเข้าสู่ระบบ";
    RowLearn => "Learn new words", "เรียนรู้คำใหม่";
    SubLearn => "Remembers words you type often or put back. Saved on this PC only.", "จำคำที่คุณพิมพ์บ่อยหรือแก้กลับ บันทึกไว้ในเครื่องนี้เท่านั้น";
    RowCaret => "Hints at the text cursor", "แสดงป้ายข้างเคอร์เซอร์";
    SubCaret => "A TH / EN tag when the language switches, and Suggest hints (Tab to use).", "ป้าย TH / EN เมื่อสลับภาษา และคำแนะนำ (กด Tab เพื่อใช้)";
    LearnedCount => "Learned words: {n}", "คำที่เรียนรู้แล้ว: {n}";
    BtnEditLearned => "Edit…", "แก้ไข…";
    HeadLanguage => "Language", "ภาษา";
    HeadHotkeys => "Hotkeys", "ปุ่มลัด";
    HeadThaiKeyboard => "Thai keyboard", "แป้นพิมพ์ไทย";
    NoteKeyboards => "English: US or UK keyboards, found automatically.", "อังกฤษ: แป้น US หรือ UK ตรวจให้อัตโนมัติ";
    NoteHotkeys => "Hotkeys work in every app. To change one, click Change and press the new keys.", "ปุ่มลัดใช้ได้ทุกแอป กด เปลี่ยน แล้วกดปุ่มใหม่ที่ต้องการ";
    HkPalette => "Open the command palette", "เปิดเมนูคำสั่ง";
    BtnChange => "Change", "เปลี่ยน";
    BtnResetKeys => "Reset all", "คืนค่าเดิม";
    HkPress => "Press the new keys for: {v} — Esc cancels.", "กดปุ่มใหม่สำหรับ: {v} — Esc เพื่อยกเลิก";
    HkUnusable => "That would get in the way of typing — use Ctrl or Alt with it.", "ปุ่มนี้จะรบกวนการพิมพ์ — ใช้ร่วมกับ Ctrl หรือ Alt";
    HkTaken => "Already used for: {v}", "ใช้กับ: {v} อยู่แล้ว";
    HkReset => "All hotkeys are back to the defaults.", "คืนปุ่มลัดทั้งหมดเป็นค่าเดิมแล้ว";
    PaletteHead => "↑↓ Enter · 1–9 · type to search · Esc", "↑↓ Enter · 1–9 · พิมพ์เพื่อค้นหา · Esc";
    PaletteFiltering => "Search: {text}", "ค้นหา: {text}";
    PalettePause => "Pause for 30 minutes", "หยุดชั่วคราว 30 นาที";
    PaletteAppOff => "Turn off in {app}", "ปิดใน {app}";
    PaletteAppOn => "Turn back on in {app}", "เปิดกลับใน {app}";
    HkFlip => "Fix or flip back the last word", "แก้หรือสลับคำล่าสุดกลับ";
    HkSelection => "Convert the selected text", "แปลงข้อความที่เลือก";
    HkCycle => "Switch correction mode", "สลับโหมดการแก้คำ";
    HkUndo => "Undo the last fix", "ย้อนการแก้ล่าสุด";
    HkAccept => "Use the suggestion", "ใช้คำแนะนำ";
    HkPanic => "Turn RightType off / on", "ปิด / เปิด RightType";
    LearnedIntro => "RightType treats these as real words and never converts them. One word per line, Thai or English — delete a line to forget that word.", "RightType ถือว่าคำเหล่านี้เป็นคำจริงและจะไม่แปลงเลย หนึ่งคำต่อบรรทัด ภาษาไทยหรืออังกฤษก็ได้ — ลบบรรทัดออกเพื่อให้ลืมคำนั้น";
    BtnSaveLearned => "Save", "บันทึก";
    BtnClearAll => "Clear all", "ล้างทั้งหมด";
    BtnImport => "Import…", "นำเข้า…";
    BtnExport => "Export…", "ส่งออก…";
    LearnedHere => "Saved on this PC only.", "บันทึกไว้ในเครื่องนี้เท่านั้น";
    LearnedInFolder => "Kept in: {v}", "เก็บไว้ที่: {v}";
    BtnSyncFolder => "Sync folder…", "โฟลเดอร์ซิงก์…";
    BtnThisPc => "This PC only", "เฉพาะเครื่องนี้";
    LearnedMoved => "{n} words, merged with any already there.", "{n} คำ รวมกับที่มีอยู่แล้ว";
    LearnedImported => "Added {n} words from the file.", "เพิ่ม {n} คำจากไฟล์แล้ว";
    LearnedExported => "Saved {n} words to the file.", "บันทึก {n} คำลงไฟล์แล้ว";
    ErrFile => "Could not use that file.", "ใช้ไฟล์นั้นไม่ได้";
    FileFilter => "Text files (*.txt)", "ไฟล์ข้อความ (*.txt)";
    LearnedSaved => "Saved {n} words.", "บันทึก {n} คำแล้ว";
    LearnedSkipped => "Saved {n} words. Skipped {k} lines that are not a single word.", "บันทึก {n} คำแล้ว ข้าม {k} บรรทัดที่ไม่ใช่คำเดียว";
    HeadBlocked => "Blocked apps", "แอปที่ไม่ทำงาน";
    HeadAppModes => "Mode per app", "โหมดของแต่ละแอป";
    AppModesIntro => "One app per line: name.exe = auto, suggest, manual or off. Quicker: tray menu → In this app.", "หนึ่งแอปต่อบรรทัด: ชื่อ.exe = auto, suggest, manual หรือ off หรือตั้งจากเมนูที่ถาดไอคอน → ในแอปนี้";
    AppsSaved => "Saved.", "บันทึกแล้ว";
    RowPredict => "Guess each field's language", "เดาภาษาของแต่ละช่อง";
    SubPredict => "Switches language as you click into a field you use for one. Counts only.", "สลับภาษาให้เมื่อคลิกเข้าช่องที่ใช้ภาษาเดียว เก็บแค่จำนวนคำ";
    BtnClearHabits => "Clear", "ล้าง";
    ToastHabitsCleared => "Field habits cleared", "ล้างข้อมูลการเดาภาษาแล้ว";
    AppsSkipped => "Saved. Skipped {k} lines that are not name.exe = mode.", "บันทึกแล้ว ข้าม {k} บรรทัดที่ไม่ใช่รูปแบบ ชื่อ.exe = โหมด";
    BlockedAlways => "RightType always stays out of password fields, terminals, password managers, crypto wallets, remote desktops and virtual machines, and full-screen games.", "RightType ไม่ทำงานในช่องรหัสผ่าน เทอร์มินัล โปรแกรมจัดการรหัสผ่าน กระเป๋าคริปโต รีโมตเดสก์ท็อปและเครื่องเสมือน และเกมเต็มจอเสมอ";
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
    BtnCheckUpdates => "Check for updates", "ตรวจสอบอัปเดต";
    AboutUpdates => "Opens the download page in your browser. RightType itself never goes online.", "เปิดหน้าดาวน์โหลดในเบราว์เซอร์ ตัว RightType เองไม่ต่ออินเทอร์เน็ต";
    BtnClose => "Close", "ปิด";

    // Welcome / help window.
    WelcomeTitle => "Welcome to RightType", "ยินดีต้อนรับสู่ RightType";
    HelpTitle => "RightType — Hotkeys", "RightType — ปุ่มลัด";
    WelcomeHeadline => "Wrong layout? Fixed — no retyping.", "พิมพ์ผิดภาษา? แก้ให้ทันที ไม่ต้องพิมพ์ใหม่";
    WelcomeSub => "Thai Kedmanee ↔ US English, in every app. Password fields, terminals and wallets are always left alone.", "ไทยเกษมณี ↔ อังกฤษ US ใช้ได้ทุกแอป และไม่ยุ่งกับช่องรหัสผ่าน เทอร์มินัล และกระเป๋าคริปโต";
    WelcomeExample => "l;ylfu  →  สวัสดี", "l;ylfu  →  สวัสดี";
    WelcomeTry => "Try it: type l;ylfu and a space on the English keyboard here", "ลองเลย: เปิดแป้นอังกฤษ แล้วพิมพ์ l;ylfu ตามด้วยเว้นวรรคในช่องนี้";
    HelpHeadline => "Hotkeys", "ปุ่มลัด";
    WelcomeMode => "Current mode: {mode}. Switch anytime with Ctrl+CapsLock.", "โหมดตอนนี้: {mode} สลับได้ทุกเมื่อด้วย Ctrl+CapsLock";
    BtnGetStarted => "Get started", "เริ่มใช้งาน";

    // Fix text window.
    FixTitle => "RightType — Fix text", "RightType — ซ่อมข้อความ";
    FixHead => "Fix text", "ซ่อมข้อความ";
    FixIntro => "Paste text typed in the wrong layout. Each word is fixed the same way RightType fixes it while you type. Nothing is saved.", "วางข้อความที่พิมพ์ผิดภาษา แล้วแต่ละคำจะถูกแก้แบบเดียวกับตอนพิมพ์ ไม่มีการบันทึกข้อความเลย";
    FixInput => "Your text", "ข้อความของคุณ";
    FixOutput => "Fixed", "ข้อความที่แก้แล้ว";
    BtnFix => "Fix", "แก้";
    BtnPasteFix => "Paste and fix", "วางแล้วแก้";
    BtnCopy => "Copy", "คัดลอก";
    FixChanged => "Words fixed ({n}):", "คำที่แก้ ({n}):";
    FixNothing => "Nothing needed fixing.", "ไม่มีคำที่ต้องแก้";
    FixCopied => "Copied — paste it where you need it.", "คัดลอกแล้ว วางได้เลย";
    ErrClipboardEmpty => "The clipboard has no text.", "คลิปบอร์ดไม่มีข้อความ";

    // Statistics window.
    StatsTitle => "RightType — Statistics", "RightType — สถิติ";
    StatsHead => "Since RightType started", "ตั้งแต่เปิดโปรแกรม";
    StatsAuto => "Fixed automatically", "แก้อัตโนมัติ";
    StatsManual => "Fixed with a hotkey", "แก้ด้วยปุ่มลัด";
    StatsLearned => "Learned words", "คำที่เรียนรู้";
    StatsSaved => "Time saved (estimate)", "เวลาที่ประหยัด (ประมาณ)";
    StatsSavedValue => "{n} min", "{n} นาที";
    StatsSavedSeconds => "{n} s", "{n} วินาที";
    StatsWeekHead => "Last 7 days", "7 วันที่ผ่านมา";
    StatsWeekTotal => "{n} words fixed this week, about {m} min saved.", "สัปดาห์นี้แก้ {n} คำ ประหยัดเวลาราว {m} นาที";
    StatsWeekOff => "Turn on daily counts below to see the last 7 days.", "เปิดการเก็บจำนวนรายวันด้านล่างเพื่อดู 7 วันที่ผ่านมา";
    RowKeepStats => "Keep daily counts on this PC", "เก็บจำนวนรายวันไว้ในเครื่องนี้";
    SubKeepStats => "Two numbers per day, never what you type. Off deletes them.", "เก็บแค่ตัวเลขวันละสองค่า ไม่เก็บสิ่งที่พิมพ์ ปิดแล้วลบทิ้ง";
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
        const SAME: &[T] = &[
            T::ToastSuggestAccept,
            T::AboutVersion,
            T::WelcomeExample,
            T::PaletteHead,
        ];
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
            for name in ["{n}", "{k}", "{v}", "{mode}", "{app}", "{m}"] {
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
