[logic]
use std::time::Duration;

fn now_time() -> String {
    chrono::Local::now().format("%H:%M:%S").to_string()
}
fn now_date() -> String {
    chrono::Local::now().format("%A, %d %B %Y").to_string()
}

let time = signal(now_time());
let date = signal(now_date());
let time_view = time.read_only();
let date_view = date.read_only();
platform_layershell::interval(Duration::from_secs(1), move || {
    time.set(now_time());
    date.set(now_date());
});

[view]
col align:center gap:8
    text "{$time_view}" size:34 color:text align:center
    text "{$date_view}" size:14 color:subtle align:center
