//! The session lock and the idle stages that lead to it.
//!
//! One `*_section` per form on the page, each owning one `[toml]` table and saving it on its own.

use std::path::Path;
use std::rc::Rc;

use telar::{Container, LayoutError, LayoutItem, LayoutStyle, signal};

use crate::form::*;
use crate::table::*;
use config::theme::NordTheme;
use config::{Config, IdleConfig, IdleStage};

/// The `[[idle.stages]]` editor. `hyprshell --list` is what the action fields accept; the placeholders name
/// the three a user reaches for.
pub(crate) fn idle_stages_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let list = Rc::new(TableList::new(config.idle.stages.clone()));

    let rows = {
        let list = Rc::clone(&list);
        let handle = Rc::clone(&list);
        handle.view(move |id| {
            let Some(stage) = list.get(id) else {
                return Ok(Box::new(Container::new(LayoutStyle::new(), vec![])?));
            };
            let (timeout, a) = bound_field(
                || telar::t!("settings.field.timeout_seconds"),
                &list,
                id,
                stage.timeout.to_string(),
                "300",
                theme,
                |entry: &mut IdleStage, text| entry.timeout = parse_u64(text, entry.timeout),
            )?;
            let (action, b) = bound_field(
                || telar::t!("settings.field.action"),
                &list,
                id,
                stage.action.clone(),
                "lock on",
                theme,
                |entry: &mut IdleStage, text| entry.action = text.to_string(),
            )?;
            let (return_action, c) = bound_field(
                || telar::t!("settings.field.return_action"),
                &list,
                id,
                stage.return_action.clone(),
                "shell dpms on",
                theme,
                |entry: &mut IdleStage, text| entry.return_action = text.to_string(),
            )?;
            entry_card(
                vec![timeout, action, return_action],
                &list,
                id,
                theme,
                vec![a, b, c],
            )
        })?
    };

    let add = {
        let list = Rc::clone(&list);
        save_button(
            || telar::t!("settings.list.add"),
            theme,
            move || list.add(IdleStage::default()),
        )?
    };

    let path = path.to_path_buf();
    let saved = Rc::clone(&list);
    let save = save_button(
        || telar::t!("settings.save.idle_stages"),
        theme,
        move || {
            persist_with(&path, "idle", |current| IdleConfig {
                stages: saved.collect(),
                ..current.idle.clone()
            });
        },
    )?;

    section(
        || telar::t!("settings.section.idle_stages"),
        vec![rows, add],
        save,
        theme,
    )
}

pub(crate) fn lock_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let l = &config.lock;
    let pam_service = signal(l.pam_service.clone());
    let max_tries = signal(l.max_tries.to_string());
    let lockout_seconds = signal(l.lockout_seconds.to_string());
    let lock_before_sleep = signal(l.lock_before_sleep);
    let fingerprint = signal(l.fingerprint);
    let howdy_command = signal(l.howdy_command.clone());
    let show_avatar = signal(l.show_avatar);
    let show_media = signal(l.show_media);
    let show_notifications = signal(l.show_notifications);
    let hide_notifs = signal(l.hide_notifs);

    let rows = vec![
        text_field(
            || telar::t!("settings.field.pam_service"),
            pam_service.clone(),
            "login",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.max_tries"),
            max_tries.clone(),
            "5",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.lockout_seconds"),
            lockout_seconds.clone(),
            "30",
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.lock_before_sleep"),
            lock_before_sleep.clone(),
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.fingerprint"),
            fingerprint.clone(),
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.howdy_command"),
            howdy_command.clone(),
            "howdy compare",
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.show_avatar"),
            show_avatar.clone(),
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.show_media"),
            show_media.clone(),
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.show_notifications"),
            show_notifications.clone(),
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.hide_notifs"),
            hide_notifs.clone(),
            theme,
        )?,
    ];

    // The keys not on the form — the library path, the biometric budgets, the weather and resource rows — are
    // carried through unchanged, so saving here never quietly drops a setting the panel has no row for.
    let base = l.clone();
    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.lock"),
        theme,
        move || {
            let value = config::LockConfig {
                pam_service: pam_service.peek().trim().to_string(),
                max_tries: parse_i32(&max_tries.peek(), base.max_tries as i32).max(0) as u32,
                lockout_seconds: parse_i32(&lockout_seconds.peek(), base.lockout_seconds as i32)
                    .max(0) as u64,
                lock_before_sleep: lock_before_sleep.peek(),
                fingerprint: fingerprint.peek(),
                howdy_command: howdy_command.peek().trim().to_string(),
                show_avatar: show_avatar.peek(),
                show_media: show_media.peek(),
                show_notifications: show_notifications.peek(),
                hide_notifs: hide_notifs.peek(),
                ..base.clone()
            };
            persist(&path, "lock", &value);
        },
    )?;
    section(|| telar::t!("settings.section.lock"), rows, save, theme)
}

pub(crate) fn idle_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let i = &config.idle;
    let enabled = signal(i.enabled);
    let inhibit_when_audio = signal(i.inhibit_when_audio);
    let inhibit_when_charging = signal(i.inhibit_when_charging);
    let respect_inhibitors = signal(i.respect_inhibitors);

    let rows = vec![
        toggle_field(
            || telar::t!("settings.field.enabled"),
            enabled.clone(),
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.inhibit_when_audio"),
            inhibit_when_audio.clone(),
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.inhibit_when_charging"),
            inhibit_when_charging.clone(),
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.respect_inhibitors"),
            respect_inhibitors.clone(),
            theme,
        )?,
    ];

    // `stages` is a list of tables, so it stays hand-edited in the TOML — K13. Carried through, so switching
    // idle on from here does not wipe the timeouts it is switching on.
    let base = i.clone();
    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.idle"),
        theme,
        move || {
            let value = config::IdleConfig {
                enabled: enabled.peek(),
                stages: base.stages.clone(),
                inhibit_when_audio: inhibit_when_audio.peek(),
                inhibit_when_charging: inhibit_when_charging.peek(),
                respect_inhibitors: respect_inhibitors.peek(),
            };
            persist(&path, "idle", &value);
        },
    )?;
    section(|| telar::t!("settings.section.idle"), rows, save, theme)
}
