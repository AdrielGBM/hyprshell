//! Ending the session: lock, log out, reboot, power off, suspend, hibernate.
//!
//! Every action goes through logind rather than through `systemctl`, for two reasons. It works without
//! privileges — logind decides what the active session's user is allowed to do — and it can be *asked* first:
//! `CanHibernate` tells the shell whether to offer hibernate at all, so the session menu greys out what this
//! machine cannot do instead of offering a button that fails.

use zbus::blocking::Connection;

const LOGIN1: &str = "org.freedesktop.login1";
const MANAGER_PATH: &str = "/org/freedesktop/login1";
const MANAGER_IFACE: &str = "org.freedesktop.login1.Manager";
const SESSION_PATH: &str = "/org/freedesktop/login1/session/auto";
const SESSION_IFACE: &str = "org.freedesktop.login1.Session";

/// What a session menu can offer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    Lock,
    Logout,
    Suspend,
    Hibernate,
    Reboot,
    Shutdown,
}

impl Action {
    pub const ALL: [Action; 6] = [
        Action::Lock,
        Action::Logout,
        Action::Suspend,
        Action::Hibernate,
        Action::Reboot,
        Action::Shutdown,
    ];

    /// The stable id used in config, IPC and the i18n catalogs.
    pub fn id(self) -> &'static str {
        match self {
            Action::Lock => "lock",
            Action::Logout => "logout",
            Action::Suspend => "suspend",
            Action::Hibernate => "hibernate",
            Action::Reboot => "reboot",
            Action::Shutdown => "shutdown",
        }
    }

    pub fn from_id(id: &str) -> Option<Action> {
        Action::ALL.into_iter().find(|a| a.id() == id)
    }

    /// The default Iconify glyph; a config may override it per action.
    pub fn icon(self) -> &'static str {
        match self {
            Action::Lock => "lock",
            Action::Logout => "log-out",
            Action::Suspend => "moon",
            Action::Hibernate => "snowflake",
            Action::Reboot => "rotate-ccw",
            Action::Shutdown => "power",
        }
    }

    /// The logind manager method, and the `Can…` property that says whether it is available. `Lock` and
    /// `Logout` act on the session object instead, so they have no manager method here.
    fn manager_method(self) -> Option<(&'static str, &'static str)> {
        match self {
            Action::Suspend => Some(("Suspend", "CanSuspend")),
            Action::Hibernate => Some(("Hibernate", "CanHibernate")),
            Action::Reboot => Some(("Reboot", "CanReboot")),
            Action::Shutdown => Some(("PowerOff", "CanPowerOff")),
            Action::Lock | Action::Logout => None,
        }
    }
}

fn connection() -> Option<Connection> {
    Connection::system().ok()
}

/// Whether logind will let this session perform `action`. The `Can…` methods answer `"yes"`, `"no"`,
/// `"challenge"` (would prompt for authentication) or `"na"` (unsupported by the machine — no swap for
/// hibernate, say). `"challenge"` counts as available: the prompt is the polkit agent's job, not the shell's.
pub fn is_available(action: Action) -> bool {
    let Some((_, probe)) = action.manager_method() else {
        return true; // lock and logout always apply to one's own session
    };
    let Some(conn) = connection() else {
        return false;
    };
    let reply: Result<String, _> = conn
        .call_method(Some(LOGIN1), MANAGER_PATH, Some(MANAGER_IFACE), probe, &())
        .and_then(|m| m.body().deserialize());
    matches!(reply.as_deref(), Ok("yes" | "challenge"))
}

/// Every action this machine can actually perform, in menu order.
pub fn available() -> Vec<Action> {
    Action::ALL
        .into_iter()
        .filter(|a| is_available(*a))
        .collect()
}

/// Performs `action`. Runs off the UI thread — a D-Bus round-trip that suspends the machine must not happen
/// inside a click handler — and reports failures rather than returning them, since a caller has nothing useful
/// to do about a refused power action beyond what logind already told the user.
pub fn perform(action: Action) {
    let _ = std::thread::Builder::new()
        .name("hyprshell-session".to_string())
        .spawn(move || {
            if let Err(e) = call(action) {
                tracing::warn!("session action '{}' failed: {e}", action.id());
            }
        });
}

fn call(action: Action) -> Result<(), zbus::Error> {
    let conn = connection().ok_or(zbus::Error::InvalidField)?;
    match action {
        // `Lock` asks the session's lock handler (us) to lock; it does not lock by itself. Emitting it rather
        // than locking directly keeps one code path whether the request came from here or from `loginctl`.
        Action::Lock => {
            conn.call_method(Some(LOGIN1), SESSION_PATH, Some(SESSION_IFACE), "Lock", &())?;
        }
        Action::Logout => {
            conn.call_method(
                Some(LOGIN1),
                SESSION_PATH,
                Some(SESSION_IFACE),
                "Terminate",
                &(),
            )?;
        }
        other => {
            let (method, _) = other.manager_method().expect("non-session actions have one");
            // `false` = don't ask the policy layer to prompt; the caller already confirmed in the menu.
            conn.call_method(
                Some(LOGIN1),
                MANAGER_PATH,
                Some(MANAGER_IFACE),
                method,
                &(false,),
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_and_cover_every_action() {
        for action in Action::ALL {
            assert_eq!(
                Action::from_id(action.id()),
                Some(action),
                "'{}' must parse back",
                action.id()
            );
            assert!(!action.icon().is_empty());
        }
        assert_eq!(Action::from_id("explode"), None);
    }

    #[test]
    fn lock_and_logout_act_on_the_session_not_the_manager() {
        assert_eq!(Action::Lock.manager_method(), None);
        assert_eq!(Action::Logout.manager_method(), None);
        assert_eq!(
            Action::Shutdown.manager_method(),
            Some(("PowerOff", "CanPowerOff"))
        );
    }

    #[test]
    fn actions_on_ones_own_session_are_always_offered() {
        // These need no capability probe, so they must not be filtered out on a machine with no system bus.
        assert!(is_available(Action::Lock));
        assert!(is_available(Action::Logout));
    }
}
