//! The installed applications, from their `.desktop` files.
//!
//! Scanned once and cached for the process: the XDG application directories hold a few hundred entries, parsing
//! them costs a few milliseconds, and a launcher that re-read them on every keystroke would be doing that work
//! hundreds of times for a list that changes when you install software. [`reload`] exists for when it does.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

/// One launchable application.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct App {
    /// The desktop-entry id (`firefox.desktop` → `firefox`), stable across restarts and what config and the
    /// launch-count store key on.
    pub id: String,
    pub name: String,
    /// `GenericName` or `Comment` — the subtitle a launcher row shows.
    pub description: String,
    /// The `Icon` key: an icon-theme name or an absolute path, for the freedesktop resolver.
    pub icon: String,
    /// `Exec` with its field codes removed.
    pub exec: String,
    pub categories: Vec<String>,
    pub keywords: Vec<String>,
    /// The entry asks to run inside a terminal emulator.
    pub terminal: bool,
}

impl App {
    /// Everything a search should match against, not just the name: `keywords` is where an entry lists the
    /// words users actually type (`www`, `browser`) and `description` catches the rest.
    pub fn haystack(&self) -> String {
        let mut text = self.name.clone();
        if !self.description.is_empty() {
            text.push(' ');
            text.push_str(&self.description);
        }
        for keyword in &self.keywords {
            text.push(' ');
            text.push_str(keyword);
        }
        text
    }
}

/// The XDG directories holding `.desktop` files, most specific first so a user override shadows the system one.
fn application_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(home) = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from) {
        dirs.push(home.join("applications"));
    } else if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        dirs.push(home.join(".local/share/applications"));
    }
    let system = std::env::var("XDG_DATA_DIRS")
        .unwrap_or_else(|_| "/usr/local/share:/usr/share".to_string());
    dirs.extend(system.split(':').filter(|s| !s.is_empty()).map(|d| PathBuf::from(d).join("applications")));
    dirs
}

/// Strips the `Exec` field codes the spec defines (`%f`, `%U`, `%i`, `%c`, `%k`, …). They stand for files or
/// URLs to open, and launching with them left in passes the literal `%U` to the program as an argument.
fn clean_exec(exec: &str) -> String {
    exec.split_whitespace()
        .filter(|word| !(word.len() == 2 && word.starts_with('%')))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Parses a desktop entry's `[Desktop Entry]` group. Returns `None` for anything not worth showing: a
/// non-application type, `NoDisplay`, `Hidden`, or an entry with no name or no command.
fn parse_entry(id: &str, text: &str) -> Option<App> {
    let mut fields: HashMap<&str, &str> = HashMap::new();
    let mut in_group = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            // Only the main group; the `[Desktop Action …]` groups that follow describe extra launch entries.
            in_group = line == "[Desktop Entry]";
            continue;
        }
        if !in_group || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            // Localised keys (`Name[es]`) are ignored in favour of the plain one; picking the right locale is
            // the shell's `locale` service's job and not worth a second parse here.
            fields.entry(key.trim()).or_insert_with(|| value.trim());
        }
    }

    if fields.get("Type").is_some_and(|t| *t != "Application") {
        return None;
    }
    if fields.get("NoDisplay").is_some_and(|v| *v == "true")
        || fields.get("Hidden").is_some_and(|v| *v == "true")
    {
        return None;
    }
    let name = fields.get("Name")?.to_string();
    let exec = clean_exec(fields.get("Exec")?);
    if name.is_empty() || exec.is_empty() {
        return None;
    }

    let split = |key: &str| -> Vec<String> {
        fields
            .get(key)
            .map(|v| {
                v.split(';')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    };

    Some(App {
        id: id.to_string(),
        name,
        description: fields
            .get("GenericName")
            .or_else(|| fields.get("Comment"))
            .unwrap_or(&"")
            .to_string(),
        icon: fields.get("Icon").unwrap_or(&"").to_string(),
        exec,
        categories: split("Categories"),
        keywords: split("Keywords"),
        terminal: fields.get("Terminal").is_some_and(|v| *v == "true"),
    })
}

/// Scans `dirs` for desktop entries. Earlier directories win, which is what makes `~/.local/share/applications`
/// override a system entry of the same id.
fn scan(dirs: &[PathBuf]) -> Vec<App> {
    let mut found: HashMap<String, App> = HashMap::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "desktop") {
                continue;
            }
            let Some(id) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if found.contains_key(id) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            if let Some(app) = parse_entry(id, &text) {
                found.insert(id.to_string(), app);
            }
        }
    }
    let mut apps: Vec<App> = found.into_values().collect();
    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    apps
}

static CACHE: OnceLock<Mutex<Vec<App>>> = OnceLock::new();

fn cache() -> &'static Mutex<Vec<App>> {
    CACHE.get_or_init(|| Mutex::new(scan(&application_dirs())))
}

/// Every installed application, sorted by name. Scanned on first call.
pub fn all() -> Vec<App> {
    cache().lock().unwrap().clone()
}

/// Re-scans the application directories — for after installing software, and for the IPC `apps reload`.
pub fn reload() -> usize {
    let apps = scan(&application_dirs());
    let count = apps.len();
    *cache().lock().unwrap() = apps;
    count
}

/// Launches `app`, detached from the shell.
///
/// Detaching matters: a child of the shell would die with it, and would inherit its file descriptors. `setsid`
/// puts the program in its own session so neither happens. A terminal entry is wrapped in the configured
/// terminal, since running it bare would give it no tty.
pub fn launch(app: &App) {
    let mut command = app.exec.clone();
    if app.terminal {
        let terminal = crate::core::shell::config()
            .map(|c| c.general.terminal.clone())
            .unwrap_or_default();
        let terminal = if terminal.trim().is_empty() {
            "xterm".to_string()
        } else {
            terminal
        };
        command = format!("{terminal} -e {command}");
    }
    crate::shared::services::state::record_launch(&app.id);
    run_detached(command);
}

/// Runs `command` through `sh -c`, detached from the shell so it outlives it and inherits none of its streams.
///
/// `setsid --fork` is what makes it detached: without it the child stays in hyprshell's session and dies with
/// it. Spawned on a thread of its own because `status()` waits for `setsid` to fork, which is short but is
/// still a `fork`/`exec` — not something to do on the frame.
pub fn run_detached(command: String) {
    let _ = std::thread::Builder::new()
        .name("hyprshell-launch".to_string())
        .spawn(move || {
            match std::process::Command::new("setsid")
                .args(["--fork", "sh", "-c", &command])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
            {
                Ok(_) => {}
                Err(e) => tracing::warn!("launching `{command}`: {e}"),
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIREFOX: &str = "\
[Desktop Entry]
Type=Application
Name=Firefox
GenericName=Web Browser
Comment=Browse the web
Exec=firefox %u
Icon=firefox
Terminal=false
Categories=Network;WebBrowser;
Keywords=Internet;WWW;Browser;

[Desktop Action new-window]
Name=New Window
Exec=firefox --new-window
";

    #[test]
    fn a_desktop_entry_parses_into_a_launchable_app() {
        let app = parse_entry("firefox", FIREFOX).expect("a normal entry parses");
        assert_eq!(app.name, "Firefox");
        assert_eq!(app.description, "Web Browser", "GenericName wins over Comment");
        assert_eq!(app.icon, "firefox");
        assert_eq!(app.exec, "firefox", "the %u field code is stripped");
        assert_eq!(app.keywords, vec!["Internet", "WWW", "Browser"]);
        assert!(!app.terminal);
    }

    #[test]
    fn the_action_groups_after_the_main_one_are_ignored() {
        // Without stopping at the group boundary, `Name=New Window` would overwrite the app's own name.
        let app = parse_entry("firefox", FIREFOX).unwrap();
        assert_eq!(app.name, "Firefox");
        assert_eq!(app.exec, "firefox");
    }

    #[test]
    fn entries_that_should_not_be_listed_are_dropped() {
        let hidden = "[Desktop Entry]\nType=Application\nName=X\nExec=x\nNoDisplay=true\n";
        assert!(parse_entry("x", hidden).is_none());

        let link = "[Desktop Entry]\nType=Link\nName=X\nURL=http://x\n";
        assert!(parse_entry("x", link).is_none(), "only applications launch");

        let no_exec = "[Desktop Entry]\nType=Application\nName=X\n";
        assert!(parse_entry("x", no_exec).is_none(), "nothing to run");

        let no_name = "[Desktop Entry]\nType=Application\nExec=x\n";
        assert!(parse_entry("x", no_name).is_none(), "nothing to show");
    }

    #[test]
    fn field_codes_are_stripped_but_real_arguments_survive() {
        assert_eq!(clean_exec("firefox %u"), "firefox");
        assert_eq!(clean_exec("env FOO=1 mpv --fs %F"), "env FOO=1 mpv --fs");
        assert_eq!(
            clean_exec("wine start /unix %f"),
            "wine start /unix",
            "only two-character %x tokens are field codes"
        );
        assert_eq!(clean_exec("prog --width=100%"), "prog --width=100%");
    }

    #[test]
    fn the_haystack_covers_the_words_users_actually_type() {
        let app = parse_entry("firefox", FIREFOX).unwrap();
        let haystack = app.haystack().to_lowercase();
        for word in ["firefox", "web browser", "www"] {
            assert!(haystack.contains(word), "'{word}' is searchable");
        }
    }

    #[test]
    fn a_user_entry_shadows_the_system_one_of_the_same_id() {
        let root = std::env::temp_dir().join(format!("hyprshell-apps-{}", std::process::id()));
        let user = root.join("user");
        let system = root.join("system");
        std::fs::create_dir_all(&user).unwrap();
        std::fs::create_dir_all(&system).unwrap();
        std::fs::write(
            user.join("editor.desktop"),
            "[Desktop Entry]\nType=Application\nName=My Editor\nExec=mine\n",
        )
        .unwrap();
        std::fs::write(
            system.join("editor.desktop"),
            "[Desktop Entry]\nType=Application\nName=System Editor\nExec=theirs\n",
        )
        .unwrap();

        let apps = scan(&[user, system]);
        assert_eq!(apps.len(), 1, "one id, one entry");
        assert_eq!(apps[0].name, "My Editor", "the user's copy wins");

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn scanning_a_missing_directory_is_not_fatal() {
        assert!(scan(&[PathBuf::from("/nonexistent-applications")]).is_empty());
    }
}
