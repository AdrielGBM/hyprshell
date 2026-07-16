[logic]
// No-op under a headless test (the clock shows its initial value there).
use std::time::Duration;

fn now_string() -> String {
    chrono::Local::now().format("%H:%M:%S").to_string()
}

let now = signal(now_string());
let now_view = now.read_only();
// module_shell provides the box, hover/press feedback and drawer-opening click; this module supplies only content, painted with the container-chosen foreground.
let fg = crate::module_fg();
platform_layershell::interval(Duration::from_secs(1), move || now.set(now_string()));

[view]
text "{$now_view}" size:14 color:$fg
