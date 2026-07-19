use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootMode {
    Desktop,
    Server,
    Tui,
}

impl BootMode {
    pub fn resolve(value: Option<&str>) -> Self {
        match value.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
            Some("server") => Self::Server,
            Some("tui") => Self::Tui,
            _ => Self::Desktop,
        }
    }

    pub fn effective(self, platform: Platform, env: &HashMap<String, String>) -> Self {
        if self != Self::Desktop {
            return self;
        }
        match platform {
            Platform::MacOs if has_any(env, &["SSH_CONNECTION", "SSH_CLIENT", "SSH_TTY"]) => {
                Self::Tui
            }
            Platform::Unix if !has_any(env, &["DISPLAY", "WAYLAND_DISPLAY"]) => Self::Tui,
            _ => Self::Desktop,
        }
    }

    pub fn components(self) -> BootComponents {
        match self {
            Self::Desktop => BootComponents {
                engine_host: true,
                ssh_daemon: false,
                desktop_ui: true,
                local_tui: false,
            },
            Self::Server => BootComponents {
                engine_host: true,
                ssh_daemon: true,
                desktop_ui: false,
                local_tui: false,
            },
            Self::Tui => BootComponents {
                engine_host: true,
                ssh_daemon: true,
                desktop_ui: false,
                local_tui: true,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    MacOs,
    Unix,
    Windows,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BootComponents {
    pub engine_host: bool,
    pub ssh_daemon: bool,
    pub desktop_ui: bool,
    pub local_tui: bool,
}

fn has_any(env: &HashMap<String, String>, keys: &[&str]) -> bool {
    keys.iter()
        .any(|key| env.get(*key).is_some_and(|value| !value.is_empty()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_resolve_and_initialize_only_their_components() {
        assert_eq!(BootMode::resolve(None), BootMode::Desktop);
        assert_eq!(BootMode::resolve(Some("SERVER")), BootMode::Server);
        assert_eq!(BootMode::resolve(Some("tui")), BootMode::Tui);
        assert!(BootMode::Desktop.components().desktop_ui);
        assert!(!BootMode::Server.components().desktop_ui);
        assert!(!BootMode::Tui.components().desktop_ui);
        assert!(BootMode::Tui.components().local_tui);
    }

    #[test]
    fn desktop_falls_back_only_when_the_platform_is_headless() {
        let empty = HashMap::new();
        assert_eq!(
            BootMode::Desktop.effective(Platform::Unix, &empty),
            BootMode::Tui
        );
        assert_eq!(
            BootMode::Desktop.effective(Platform::MacOs, &empty),
            BootMode::Desktop
        );
        assert_eq!(
            BootMode::Desktop.effective(Platform::Windows, &empty),
            BootMode::Desktop
        );
        let display = HashMap::from([("DISPLAY".into(), ":0".into())]);
        assert_eq!(
            BootMode::Desktop.effective(Platform::Unix, &display),
            BootMode::Desktop
        );
        let ssh = HashMap::from([("SSH_TTY".into(), "/dev/tty".into())]);
        assert_eq!(
            BootMode::Desktop.effective(Platform::MacOs, &ssh),
            BootMode::Tui
        );
        assert_eq!(
            BootMode::Server.effective(Platform::Unix, &empty),
            BootMode::Server
        );
    }
}
