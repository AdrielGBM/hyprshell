//! Running another program and waiting for it, without ever waiting for ever.
//!
//! `Command::output` blocks until the child exits, which for a shell means one wedged helper parks the thread that
//! was calling it — and every question queued behind it. Three callers wanted the same deadline (`qalc`, `ddcutil`
//! twice over), so the wait lives here once.
//!
//! Only ever called off the UI thread: even with a deadline, this is a process start.

use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// How often the wait looks in on the child. Coarse on purpose — nothing is waiting on this thread but a reading.
const POLL: Duration = Duration::from_millis(20);

/// The one place in the tree a child process is constructed.
///
/// Everything else goes through [`deps::command`](crate::deps::command), which takes a declared dependency
/// rather than a name — so the list of what this shell reaches for cannot be incomplete. This raw form is for
/// the two things that are *not* dependencies: a command the **user** wrote (a launcher action, a scheme hook,
/// the configured annotator or `howdy` line), and the helpers in this module. `a_process_is_only_started_through_this_module`
/// is what keeps that true.
pub fn command(program: &str) -> Command {
    Command::new(program)
}

/// Launches `command` through a shell and forgets about it — `setsid --fork` so it survives the shell exiting,
/// and every stream nulled so it can neither block on a pipe nor write over the shell's own output.
///
/// The opposite trade to [`output`]: nothing here reads a result, so the child is disowned rather than waited on.
pub fn run_detached(line: String) {
    let _ = std::thread::Builder::new()
        .name("hyprshell-launch".to_string())
        .spawn(move || {
            match command("setsid")
                .args(["--fork", "sh", "-c", &line])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
            {
                Ok(_) => {}
                Err(e) => tracing::warn!("launching `{line}`: {e}"),
            }
        });
}

/// Runs `program args…` and returns its standard output.
///
/// `None` covers every way this can fail to produce an answer — the program is not installed, it exited non-zero,
/// or it outstayed `timeout` and was killed — because a caller reading a value has the same fallback for all
/// three. What it must never do is return late.
pub fn output(program: &str, args: &[&str], timeout: Duration) -> Option<String> {
    let mut child = command(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break,
            Ok(Some(_)) => return None,
            Ok(None) if Instant::now() >= deadline => {
                tracing::warn!("{program} did not answer within {timeout:?}");
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => std::thread::sleep(POLL),
            Err(_) => return None,
        }
    }

    // Read after exit: these callers ask for a line or two, which fits the pipe buffer many times over, so there
    // is no producer left blocked on a full pipe to deadlock against.
    let mut text = String::new();
    child.stdout.take()?.read_to_string(&mut text).ok()?;
    Some(text)
}

/// Whether `program` is on the `PATH` at all, asked by running it with `args` (usually a `--version`).
///
/// A missing helper is the common case on a machine that simply does not have it, and the answer decides whether a
/// service bothers to start — so it is worth one cheap call rather than a failure per reading.
pub fn available(program: &str, args: &[&str], timeout: Duration) -> bool {
    output(program, args, timeout).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_program_that_is_not_there_answers_none_rather_than_waiting() {
        assert_eq!(
            output(
                "hyprshell-no-such-program-9e3f",
                &[],
                Duration::from_secs(1)
            ),
            None
        );
        assert!(!available(
            "hyprshell-no-such-program-9e3f",
            &["--version"],
            Duration::from_secs(1)
        ));
    }

    #[test]
    fn stdout_comes_back_and_a_failure_does_not() {
        assert_eq!(
            output("true", &[], Duration::from_secs(2)).as_deref(),
            Some("")
        );
        assert_eq!(output("false", &[], Duration::from_secs(2)), None);
        assert_eq!(
            output("echo", &["hello"], Duration::from_secs(2)).as_deref(),
            Some("hello\n")
        );
    }

    /// The guard that keeps [`crate::deps::ALL`] complete.
    ///
    /// A registry is only the source of truth if it cannot be bypassed, and in Rust nothing stops a new call
    /// site reaching for `Command::new` directly — at which point the dependency panel goes on cheerfully
    /// reporting a list that is missing the program the shell just failed to find. So this walks the tree and
    /// insists that constructing a child happens here, where `deps::command` can be the front door.
    ///
    /// Two spellings are allowed through, and both are deliberate: this module's own use, and
    /// `process::command(…)` at a site that runs a command the **user** wrote rather than one the shell
    /// depends on — a launcher action, a scheme hook, the configured annotator or `howdy` line. Those have no
    /// row because there is nothing stable to put in one.
    #[test]
    fn a_process_is_only_started_through_this_module() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(std::path::Path::parent)
            .expect("the workspace root is two levels above this crate")
            .to_path_buf();
        let mut offenders = Vec::new();
        let mut stack = vec![root.join("crates"), root.join("apps")];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = dir.read_dir() else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                // `.telar/build` is the transpiler's own output, not source.
                if path.is_dir() {
                    if !matches!(path.file_name().and_then(|n| n.to_str()), Some(".telar")) {
                        stack.push(path);
                    }
                    continue;
                }
                let is_source = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e == "rs" || e == "rsx");
                if !is_source || path == std::path::Path::new(file!()) {
                    continue;
                }
                if path.ends_with("util/src/process.rs") {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                if text.contains("Command::new") {
                    offenders.push(
                        path.strip_prefix(&root)
                            .unwrap_or(&path)
                            .display()
                            .to_string(),
                    );
                }
            }
        }
        offenders.sort();
        assert!(
            offenders.is_empty(),
            "these start a process without declaring it — use `deps::command(Dep::…)`, or \
             `process::command` if it is a command the user wrote: {offenders:#?}"
        );
    }

    /// The reason this module exists: a child that never exits must not hold the thread.
    #[test]
    fn a_child_that_will_not_finish_is_killed_at_the_deadline() {
        let started = Instant::now();
        assert_eq!(output("sleep", &["30"], Duration::from_millis(120)), None);
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the wait returned after {:?}, so the deadline did nothing",
            started.elapsed()
        );
    }
}
