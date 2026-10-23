//! The icon of each page, setting and action, in one place, so the same thing
//! looks the same in every window.
//!
//! Each is a glyph of Windows' icon font (Segoe Fluent Icons on Windows 11,
//! Segoe MDL2 Assets on Windows 10). Only code points that both fonts have,
//! drawn alike, are listed; `tools/check_icons.py` checks that. Where the font
//! is missing (`ui::icon_face()` is `None`) callers keep their text.
//!
//! Rules for using them:
//! - one icon per main row or button, before the text, never instead of the
//!   only label of something that has no tooltip;
//! - the same icon for the same thing everywhere (the Learned words page, its
//!   switch and its menu entry share `LEARNED`);
//! - the colour comes from the palette, so the icons follow the dark, light
//!   and high contrast themes.
#![allow(dead_code)] // The pages pick these up one at a time while the UI is redone.

// --- Settings pages (the sidebar) -----------------------------------------
pub const GENERAL: char = '\u{E713}'; // Settings
pub const KEYBOARD: char = '\u{E765}'; // Keyboard
pub const TOOLS: char = '\u{E90F}'; // Repair (a wrench, as in the palette)
pub const HOTKEYS: char = '\u{E945}'; // Lightning
pub const LEARNED: char = '\u{E82D}'; // Library (a book)
pub const SNIPPETS: char = '\u{E943}'; // Code (braces)
pub const APPS: char = '\u{E7F4}'; // TVMonitor (as "Here" in the palette)
pub const PRIVACY: char = '\u{E72E}'; // Lock
pub const SUPPORT: char = '\u{EB51}'; // Heart

// --- Switches on the General and Keyboard pages ----------------------------
pub const POWER: char = '\u{E7E8}'; // PowerButton
pub const START_WITH_WINDOWS: char = '\u{E768}'; // Play
pub const CURSOR_HINTS: char = '\u{E8B0}'; // Cursor
pub const SPELLING: char = '\u{E775}'; // Language
pub const WEB_ADDRESSES: char = '\u{E774}'; // Globe
pub const TAB_COMPLETE: char = '\u{E8A0}'; // Tab (an arrow to a bar)
pub const LANGUAGE_SWITCH: char = '\u{E8AB}'; // Switch
pub const UNDO: char = '\u{E7A7}'; // Undo
pub const HOLD_KEY: char = '\u{E8D2}'; // Font size
pub const SHORTCUTS: char = '\u{E945}'; // Lightning
pub const INFO: char = '\u{E946}'; // Info
pub const NUMPAD: char = '\u{E8EF}'; // Calculator
pub const INSERT_KEY: char = '\u{E70F}'; // Edit
pub const SCANNER: char = '\u{E950}'; // Processing (a chip)
pub const SHIELD: char = '\u{E730}'; // Shield
pub const ON_SCREEN: char = '\u{E7F0}'; // ClosedCaption
pub const REST: char = '\u{E823}'; // Clock
pub const MATH: char = '\u{E94A}'; // Divide (symbol suggestions)
pub const GUESS: char = '\u{E7B3}'; // View (guessing the field's language)
pub const RESTART: char = '\u{E777}'; // Refresh (start again)
pub const MODE_AUTO: char = '\u{E945}'; // Lightning (as in the palette)
pub const MODE_SUGGEST: char = '\u{E82F}'; // Lightbulb
pub const MODE_MANUAL: char = '\u{E765}'; // Keyboard
pub const MODE_CODE: char = '\u{E943}'; // Code (braces)
pub const MODE_OFF: char = '\u{E7E8}'; // PowerButton
pub const TOOL_CLEAN: char = '\u{E72E}'; // Lock (the keyboard is locked while it is wiped)
pub const TOOL_TEST: char = '\u{E765}'; // Keyboard
pub const TOOL_MAP: char = '\u{E8A9}'; // ViewAll (the keys laid out)
pub const TOOL_PRACTICE: char = '\u{E7BE}'; // Education (a graduation cap)
pub const MODE_SECTION: char = '\u{E9E9}'; // Equalizer (the palette's Mode section)
pub const HEALTH: char = '\u{E95E}'; // HeartPulse
pub const MOUSE: char = '\u{E962}'; // Mouse
pub const SCORE: char = '\u{E9D9}'; // Chart

// --- Actions ---------------------------------------------------------------
pub const ADD: char = '\u{E710}'; // Add
pub const REMOVE: char = '\u{E74D}'; // Delete
pub const EDIT: char = '\u{E70F}'; // Edit
pub const SAVE: char = '\u{E74E}'; // Save
pub const COPY: char = '\u{E8C8}'; // Copy
pub const REFRESH: char = '\u{E72C}'; // Refresh
pub const IMPORT: char = '\u{E896}'; // Download
pub const EXPORT: char = '\u{E898}'; // Upload
pub const FOLDER: char = '\u{E8B7}'; // Folder
pub const OPEN_LINK: char = '\u{E8A7}'; // OpenInNewWindow
pub const SEARCH: char = '\u{E721}'; // Search
pub const PAUSE: char = '\u{E769}'; // Pause
pub const CHECK: char = '\u{E73E}'; // Accept
pub const WARNING: char = '\u{E7BA}'; // Warning
pub const CLOSE: char = '\u{E711}'; // Cancel
pub const BACK: char = '\u{E76B}'; // ChevronLeft
pub const CHEVRON: char = '\u{E76C}'; // ChevronRight
pub const PASTE: char = '\u{E77F}'; // Paste
pub const CALENDAR: char = '\u{E787}'; // Calendar
pub const PIN: char = '\u{E718}'; // Pin
pub const UNLOCK: char = '\u{E785}'; // Unlock
