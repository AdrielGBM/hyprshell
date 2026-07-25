[logic]
// No-op under a headless test (the clock shows its initial value there).
use crate::shared::services::clock;
use crate::shared::theme::{FontRole, NordTheme};

fn now_string() -> String {
    chrono::Local::now().format("%H:%M:%S").to_string()
}

let now = signal(now_string());
let now_view = now.read_only();
// module_shell provides the box, hover/press feedback and drawer-opening click; this module supplies only content, painted with the container-chosen foreground.
let fg = crate::module_fg();
let body = use_theme::<NordTheme>().font(FontRole::Body);
// One ticker for the whole shell, aligned to the second boundary; every clock surface reads the same broadcast.
platform_layershell::watch(clock::subscribe, move |t: clock::Now| {
    now.set(t.format("%H:%M:%S").to_string());
});

[view]
text "{$now_view}" size:body color:$fg
