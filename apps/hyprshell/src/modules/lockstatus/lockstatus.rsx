[logic]
use crate::modules::lockstatus::{indicator, shown};
use crate::shared::services::lockkeys::{self, LockKeys};
use crate::shared::theme::NordTheme;

let config = crate::surface_env()
    .map(|env| env.config.lock_status)
    .unwrap_or_default();

let keys = signal(lockkeys::current().unwrap_or_else(lockkeys::read));
let listed = keys.read_only();
let tint = keys.read_only();

platform_layershell::watch(lockkeys::subscribe, move |state: LockKeys| keys.set(state));

// Which indicators exist is itself reactive under `hide_inactive`, so the row's children are a keyed list
// rather than a fixed pair — an indicator appearing or leaving never rebuilds the other one.
let indicators = memo(move || shown(listed.get(), config));

let fg = crate::module_fg();
let idle = use_theme::<NordTheme>().muted;
let size = icon_px();
let gap = (size * 0.25).round();

[view]
row align:center gap:gap
    for lock in $indicators key *lock
        build "indicator(lock, tint.clone(), fg.clone(), idle, size)?"
