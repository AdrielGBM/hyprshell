[logic]
// No-op under a headless test (the clock shows its initial value there).
use std::time::Duration;

fn now_string() -> String {
    chrono::Local::now().format("%H:%M:%S").to_string()
}

let now = signal(now_string());
let now_view = now.read_only();
platform_layershell::interval(Duration::from_secs(1), move || now.set(now_string()));

[view]
box on_press(|| crate::toggle_drawer("clock")) pad_x:8 pad_y:4 align:center justify:center
    text "{$now_view}" size:14 color:text
