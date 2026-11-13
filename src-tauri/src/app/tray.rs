//! The menu-bar icon (§4.1): a status mark with a red dot while recording, and a
//! localized menu whose checks follow the settings.

use std::sync::Mutex;
use std::time::Duration;

use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, Wry};

use super::commands::{update_settings, AppState};
use super::orchestrator::AppStatus;
use super::settings_window;
use crate::settings::{RecordMode, Settings, TranslateStyle, UiLanguage};

pub const SIZE: usize = 36;
const TRAY_ID: &str = "main";
const BLINK: Duration = Duration::from_millis(500);
const RED: [u8; 3] = [0xF2, 0x55, 0x5A];
const DIM_ALPHA: u8 = 90;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayVisual {
    Idle,
    Recording,
    Processing { dim: bool },
}

/// Coverage (0..1) of a vertical capsule of radius `r` between `y0` and `y1` at pixel (x, y).
fn capsule(x: f32, y: f32, cx: f32, y0: f32, y1: f32, r: f32) -> f32 {
    let py = y.clamp(y0, y1);
    let d = ((x - cx).powi(2) + (y - py).powi(2)).sqrt();
    (r + 0.5 - d).clamp(0.0, 1.0)
}

/// A temporary mark until the brand kit (§5.5): five rounded bars (12/22/30/22/12 px)
/// on a 36×36 canvas (18 pt @2x), plus a red dot at (30, 30) while recording/processing.
pub fn render_icon(v: TrayVisual, dark_menu_bar: bool) -> Vec<u8> {
    let glyph: u8 = if v != TrayVisual::Idle && dark_menu_bar { 255 } else { 0 };
    let dot_alpha = match v {
        TrayVisual::Idle => None,
        TrayVisual::Recording | TrayVisual::Processing { dim: false } => Some(255.0),
        TrayVisual::Processing { dim: true } => Some(f32::from(DIM_ALPHA)),
    };
    let mut img = vec![0u8; SIZE * SIZE * 4];
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let mut a = 0f32;
            for (i, h) in [12.0f32, 22.0, 30.0, 22.0, 12.0].into_iter().enumerate() {
                let cx = 4.0 + 7.0 * i as f32;
                a = a.max(capsule(fx, fy, cx, 18.0 - h / 2.0 + 2.0, 18.0 + h / 2.0 - 2.0, 2.0));
            }
            let o = (y * SIZE + x) * 4;
            if let Some(alpha) = dot_alpha {
                let d = ((fx - 30.0).powi(2) + (fy - 30.0).powi(2)).sqrt();
                // A clear ring keeps the dot readable where it meets a bar.
                if d < 6.5 {
                    a = 0.0;
                }
                let cover = (5.0 + 0.5 - d).clamp(0.0, 1.0);
                if cover > 0.0 {
                    img[o..o + 3].copy_from_slice(&RED);
                    img[o + 3] = (alpha * cover).round() as u8;
                    continue;
                }
            }
            if a > 0.0 {
                img[o..o + 3].copy_from_slice(&[glyph; 3]);
                img[o + 3] = (255.0 * a).round() as u8;
            }
        }
    }
    img
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Ru,
    En,
}

/// "Auto" follows the system: Russian if it starts with `ru`, otherwise English.
pub fn resolve_lang(ui: UiLanguage, system: Option<&str>) -> Lang {
    match ui {
        UiLanguage::Ru => Lang::Ru,
        UiLanguage::En => Lang::En,
        UiLanguage::Auto if system.is_some_and(|s| s.to_lowercase().starts_with("ru")) => Lang::Ru,
        UiLanguage::Auto => Lang::En,
    }
}

pub struct TrayLabels {
    pub status_idle: &'static str,
    pub status_recording: &'static str,
    pub status_processing: &'static str,
    pub record_mode: &'static str,
    pub hold: &'static str,
    pub toggle: &'static str,
    pub translate_style: &'static str,
    pub business: &'static str,
    pub casual: &'static str,
    pub history: &'static str,
    pub settings: &'static str,
    pub check_updates: &'static str,
    /// "{v}" is replaced with the version.
    pub update_to: &'static str,
    /// Silence from the agents (spec 2026-11-09 §3.5).
    pub agents_pause: &'static str,
    pub quit: &'static str,
}

/// The tray menu is the only UI text that lives in Rust (plan: Global Constraints).
pub fn labels(lang: Lang) -> TrayLabels {
    match lang {
        Lang::Ru => TrayLabels {
            status_idle: "Готов к записи",
            status_recording: "Идёт запись…",
            status_processing: "Обработка…",
            record_mode: "Режим записи",
            hold: "Удержание",
            toggle: "Нажатие",
            translate_style: "Стиль перевода",
            business: "Деловой",
            casual: "Разговорный",
            history: "История",
            settings: "Настройки…",
            check_updates: "Проверить обновления",
            update_to: "Обновить до {v}…",
            agents_pause: "Пауза агентов",
            quit: "Выйти из Utter",
        },
        Lang::En => TrayLabels {
            status_idle: "Ready",
            status_recording: "Recording…",
            status_processing: "Processing…",
            record_mode: "Recording mode",
            hold: "Hold",
            toggle: "Toggle",
            translate_style: "Translation style",
            business: "Business",
            casual: "Casual",
            history: "History",
            settings: "Settings…",
            check_updates: "Check for updates",
            update_to: "Update to {v}…",
            agents_pause: "Pause agents",
            quit: "Quit Utter",
        },
    }
}

/// «Пауза агентов» is in the menu only while an agent is connected.
pub fn pause_shown(s: &Settings) -> bool {
    s.agents.claude_code || s.agents.codex
}

/// Where «Пауза агентов» goes in the menu: after «Проверить обновления».
const PAUSE_AT: usize = 8;

fn lang_of(s: &Settings) -> Lang {
    resolve_lang(s.ui_language, sys_locale::get_locale().as_deref())
}

/// Menu items kept so their text and checks can change later.
pub struct TrayHandles {
    status: MenuItem<Wry>,
    mode: Submenu<Wry>,
    hold: CheckMenuItem<Wry>,
    toggle: CheckMenuItem<Wry>,
    style: Submenu<Wry>,
    business: CheckMenuItem<Wry>,
    casual: CheckMenuItem<Wry>,
    history: MenuItem<Wry>,
    settings: MenuItem<Wry>,
    updates: MenuItem<Wry>,
    menu: Menu<Wry>,
    pause: CheckMenuItem<Wry>,
    /// The version a found update offers (the item then reads "Update to …").
    offer: Mutex<Option<String>>,
    quit: MenuItem<Wry>,
    /// Each status change gets a generation; a blink loop stops once a newer one exists.
    gen: Mutex<(u64, AppStatus)>,
}

pub fn build(app: &AppHandle, s: &Settings) -> tauri::Result<()> {
    let l = labels(lang_of(s));
    let status = MenuItem::with_id(app, "status", l.status_idle, false, None::<&str>)?;
    let hold = CheckMenuItem::with_id(app, "mode_hold", l.hold, true, s.record_mode == RecordMode::Hold, None::<&str>)?;
    let toggle = CheckMenuItem::with_id(app, "mode_toggle", l.toggle, true, s.record_mode == RecordMode::Toggle, None::<&str>)?;
    let mode = Submenu::with_id_and_items(app, "mode", l.record_mode, true, &[&hold, &toggle])?;
    let business =
        CheckMenuItem::with_id(app, "style_business", l.business, true, s.translate_style == TranslateStyle::Business, None::<&str>)?;
    let casual = CheckMenuItem::with_id(app, "style_casual", l.casual, true, s.translate_style == TranslateStyle::Casual, None::<&str>)?;
    let style = Submenu::with_id_and_items(app, "style", l.translate_style, true, &[&business, &casual])?;
    let history = MenuItem::with_id(app, "history", l.history, true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", l.settings, true, None::<&str>)?;
    let updates = MenuItem::with_id(app, "updates", l.check_updates, true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", l.quit, true, None::<&str>)?;
    // Pause does not survive a restart (spec §3.5): it always starts unchecked.
    let pause = CheckMenuItem::with_id(app, "agents_pause", l.agents_pause, true, false, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &status,
            &PredefinedMenuItem::separator(app)?,
            &mode,
            &style,
            &PredefinedMenuItem::separator(app)?,
            &history,
            &settings,
            &updates,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;
    if pause_shown(s) {
        menu.insert(&pause, PAUSE_AT)?;
    }
    TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .show_menu_on_left_click(true)
        .icon(Image::new_owned(render_icon(TrayVisual::Idle, false), SIZE as u32, SIZE as u32))
        .icon_as_template(true)
        .on_menu_event(|app, event| on_menu(app, event.id.as_ref()))
        .build(app)?;
    app.manage(TrayHandles {
        status,
        mode,
        hold,
        toggle,
        style,
        business,
        casual,
        history,
        settings,
        updates,
        menu,
        pause,
        offer: Mutex::new(None),
        quit,
        gen: Mutex::new((0, AppStatus::Idle)),
    });
    Ok(())
}

fn on_menu(app: &AppHandle, id: &str) {
    match id {
        "settings" => settings_window::open(app, None),
        "history" => settings_window::open(app, Some("history")),
        "updates" => settings_window::open(app, Some("about")),
        "quit" => app.exit(0),
        "agents_pause" => {
            let on = app.try_state::<TrayHandles>().and_then(|h| h.pause.is_checked().ok()).unwrap_or(false);
            crate::agents::service::set_paused(app, on);
        }
        "mode_hold" | "mode_toggle" | "style_business" | "style_casual" => {
            let Some(state) = app.try_state::<AppState>() else { return };
            let mut next = state.shared.settings();
            match id {
                "mode_hold" => next.record_mode = RecordMode::Hold,
                "mode_toggle" => next.record_mode = RecordMode::Toggle,
                "style_business" => next.translate_style = TranslateStyle::Business,
                _ => next.translate_style = TranslateStyle::Casual,
            }
            if let Err(e) = update_settings(app, &state, next) {
                tracing::warn!("tray settings change: {e}");
                // A click toggles the check by itself; put it back to the saved value.
                refresh(app, &state.shared.settings());
            }
        }
        _ => {}
    }
}

/// Menu checks and language from the settings (called by `update_settings`).
pub fn refresh(app: &AppHandle, s: &Settings) {
    let Some(h) = app.try_state::<TrayHandles>() else { return };
    let l = labels(lang_of(s));
    let _ = h.hold.set_checked(s.record_mode == RecordMode::Hold);
    let _ = h.toggle.set_checked(s.record_mode == RecordMode::Toggle);
    let _ = h.business.set_checked(s.translate_style == TranslateStyle::Business);
    let _ = h.casual.set_checked(s.translate_style == TranslateStyle::Casual);
    let _ = h.mode.set_text(l.record_mode);
    let _ = h.hold.set_text(l.hold);
    let _ = h.toggle.set_text(l.toggle);
    let _ = h.style.set_text(l.translate_style);
    let _ = h.business.set_text(l.business);
    let _ = h.casual.set_text(l.casual);
    let _ = h.history.set_text(l.history);
    let _ = h.settings.set_text(l.settings);
    let offer = h.offer.lock().ok().and_then(|o| o.clone());
    let _ = h.updates.set_text(updates_text(&l, offer.as_deref()));
    let _ = h.quit.set_text(l.quit);
    let _ = h.pause.set_text(l.agents_pause);
    // In the menu exactly while an agent is connected; leaving it switches the pause off.
    let present = h.menu.items().map(|items| items.iter().any(|i| i.id() == h.pause.id())).unwrap_or(false);
    if pause_shown(s) && !present {
        let _ = h.menu.insert(&h.pause, PAUSE_AT);
    } else if !pause_shown(s) && present {
        let _ = h.menu.remove(&h.pause);
        if h.pause.is_checked().unwrap_or(false) {
            let _ = h.pause.set_checked(false);
            crate::agents::service::set_paused(app, false);
        }
    }
    let status = h.gen.lock().map(|g| g.1).unwrap_or(AppStatus::Idle);
    let _ = h.status.set_text(status_text(&l, status));
}

/// "Check for updates", or "Update to {v}…" once an update is found.
fn updates_text(l: &TrayLabels, offer: Option<&str>) -> String {
    match offer {
        Some(v) => l.update_to.replace("{v}", v),
        None => l.check_updates.to_owned(),
    }
}

/// What Utter is doing now (recording, processing, idle) — the tray keeps the latest status.
pub fn current_status(app: &AppHandle) -> AppStatus {
    app.try_state::<TrayHandles>().and_then(|h| h.gen.lock().ok().map(|g| g.1)).unwrap_or(AppStatus::Idle)
}

/// Called by the updater when an update is found (or no longer offered).
pub fn set_update(app: &AppHandle, version: Option<&str>) {
    let Some(h) = app.try_state::<TrayHandles>() else { return };
    if let Ok(mut o) = h.offer.lock() {
        *o = version.map(str::to_owned);
    }
    let lang = app.try_state::<AppState>().map(|s| lang_of(&s.shared.settings())).unwrap_or(Lang::En);
    let _ = h.updates.set_text(updates_text(&labels(lang), version));
}

fn status_text(l: &TrayLabels, s: AppStatus) -> &'static str {
    match s {
        AppStatus::Idle => l.status_idle,
        AppStatus::Recording => l.status_recording,
        AppStatus::Processing => l.status_processing,
    }
}

/// The menu bar follows the system appearance; the overlay window's theme is never forced.
fn dark_menu_bar(app: &AppHandle) -> bool {
    app.get_webview_window("overlay").and_then(|w| w.theme().ok()) == Some(tauri::Theme::Dark)
}

fn draw(app: &AppHandle, v: TrayVisual) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else { return };
    let img = Image::new_owned(render_icon(v, dark_menu_bar(app)), SIZE as u32, SIZE as u32);
    let _ = tray.set_icon(Some(img));
    // Only the plain mark can be a template image: the dot must stay red.
    let _ = tray.set_icon_as_template(v == TrayVisual::Idle);
}

/// Icon and status line; while processing the dot blinks every 500 ms until the status changes.
pub fn set_status(app: &AppHandle, s: AppStatus) {
    let Some(h) = app.try_state::<TrayHandles>() else { return };
    let gen = match h.gen.lock() {
        Ok(mut g) => {
            g.0 += 1;
            g.1 = s;
            g.0
        }
        Err(_) => return,
    };
    let settings = app.try_state::<AppState>().map(|st| st.shared.settings()).unwrap_or_default();
    let _ = h.status.set_text(status_text(&labels(lang_of(&settings)), s));
    match s {
        AppStatus::Idle => draw(app, TrayVisual::Idle),
        AppStatus::Recording => draw(app, TrayVisual::Recording),
        AppStatus::Processing => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let mut dim = false;
                loop {
                    let current = app.try_state::<TrayHandles>().and_then(|h| h.gen.lock().ok().map(|g| g.0 == gen));
                    if current != Some(true) {
                        break;
                    }
                    draw(&app, TrayVisual::Processing { dim });
                    dim = !dim;
                    tokio::time::sleep(BLINK).await;
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px(img: &[u8], x: usize, y: usize) -> [u8; 4] {
        let i = (y * SIZE + x) * 4;
        [img[i], img[i + 1], img[i + 2], img[i + 3]]
    }

    #[test]
    fn updates_item_offers_the_found_version() {
        let ru = labels(Lang::Ru);
        assert_eq!(updates_text(&ru, None), "Проверить обновления");
        assert_eq!(updates_text(&ru, Some("1.2.0")), "Обновить до 1.2.0…");
        assert_eq!(updates_text(&labels(Lang::En), Some("1.2.0")), "Update to 1.2.0…");
    }

    #[test]
    fn idle_icon_has_no_red() {
        let img = render_icon(TrayVisual::Idle, true);
        assert!(img.chunks(4).all(|p| !(p[0] > 200 && p[1] < 120 && p[3] > 0)));
        assert_eq!(px(&img, 18, 18)[3], 255, "the mark is drawn");
    }

    #[test]
    fn recording_icon_has_red_dot() {
        assert_eq!(px(&render_icon(TrayVisual::Recording, false), 30, 30), [0xF2, 0x55, 0x5A, 255]);
    }

    #[test]
    fn processing_dim_frame_is_translucent() {
        assert_eq!(px(&render_icon(TrayVisual::Processing { dim: true }, false), 30, 30)[3], 90);
        assert_eq!(px(&render_icon(TrayVisual::Processing { dim: false }, false), 30, 30)[3], 255);
    }

    #[test]
    fn labels_follow_language() {
        assert_eq!(resolve_lang(UiLanguage::Auto, Some("ru-RU")), Lang::Ru);
        assert_eq!(resolve_lang(UiLanguage::Auto, Some("de-DE")), Lang::En);
        assert_eq!(resolve_lang(UiLanguage::Auto, None), Lang::En);
        assert_eq!(resolve_lang(UiLanguage::Ru, Some("en-US")), Lang::Ru);
        assert_eq!(labels(Lang::En).quit, "Quit Utter");
        assert_eq!(labels(Lang::Ru).settings, "Настройки…");
    }

    #[test]
    fn menu_has_pause_when_agent_connected() {
        let mut s = Settings::default();
        assert!(!pause_shown(&s));
        s.agents.codex = true;
        assert!(pause_shown(&s));
        s.agents = crate::settings::AgentSettings { claude_code: true, codex: false };
        assert!(pause_shown(&s));
        assert_eq!(labels(Lang::Ru).agents_pause, "Пауза агентов");
        assert_eq!(labels(Lang::En).agents_pause, "Pause agents");
    }

    #[test]
    fn dark_menu_bar_draws_white_glyph() {
        assert_eq!(px(&render_icon(TrayVisual::Recording, true), 18, 18), [255, 255, 255, 255]);
        assert_eq!(px(&render_icon(TrayVisual::Recording, false), 18, 18), [0, 0, 0, 255]);
    }
}
