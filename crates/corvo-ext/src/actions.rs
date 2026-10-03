//! The Actions module: builders for a result's actions menu.
//!
//! A menu is a `Vec<CommandAction>` returned from
//! `Command::actions(result_id)`. These builders cover the built-in
//! shapes most extensions need, mapped onto the `Action` variants the
//! UI interprets: Copy, PasteText, Open, OpenUrl, RunShell, RunProcess,
//! ShowToast, TerminateProcess, and the window/system actions.

use corvo_core::{Action, ActionGroup, CommandAction, Icon};

fn action(id: &str, label: &str, action: Action, icon: Icon, group: ActionGroup) -> CommandAction {
    CommandAction {
        id: id.to_owned(),
        label: label.to_owned(),
        action,
        icon,
        group,
        hotkey: None,
    }
}

/// Copy `text` to the clipboard. Prefer this over a shell `pbcopy`
/// chain — the UI handles the platform clipboard and feedback.
pub fn copy(id: &str, text: impl Into<String>) -> CommandAction {
    action(
        id,
        "Copy",
        Action::Copy(text.into()),
        Icon::Clipboard,
        ActionGroup::Standard,
    )
}

/// Open a URL with the system handler (browser, mail, app store...).
pub fn open_url(id: &str, url: impl Into<String>) -> CommandAction {
    action(
        id,
        "Open URL",
        Action::OpenUrl(url.into()),
        Icon::Web,
        ActionGroup::Standard,
    )
}

/// Open a file or folder with the system handler.
pub fn open_path(id: &str, path: impl Into<String>) -> CommandAction {
    action(
        id,
        "Open",
        Action::Open(path.into().into()),
        Icon::File,
        ActionGroup::Standard,
    )
}

/// Reveal a path in the platform file manager (Finder, Explorer, or
/// the file manager `xdg-open` picks).
pub fn reveal_path(id: &str, path: &std::path::Path) -> CommandAction {
    let path = path.to_string_lossy();
    let shell_command = if cfg!(target_os = "macos") {
        format!("open -R {path}")
    } else if cfg!(target_os = "windows") {
        format!("explorer.exe /select,\"{path}\"")
    } else {
        let parent = std::path::Path::new(path.as_ref())
            .parent()
            .map(|parent| parent.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string());
        format!("xdg-open {parent}")
    };
    action(
        id,
        "Reveal in Files",
        Action::RunShell(shell_command),
        Icon::File,
        ActionGroup::Standard,
    )
}

/// An extension-specific action. Express it as a concrete `Action`
/// the UI can run directly — for example
/// `Action::RunProcess { program, args, title }` to invoke a CLI. The
/// `id` must still be namespaced `{command-id}:{action}[:{data}]`.
pub fn custom(id: &str, label: &str, run: Action, icon: Icon) -> CommandAction {
    action(id, label, run, icon, ActionGroup::Standard)
}

/// A removal-class action. The UI renders it apart, in red.
pub fn destructive(id: &str, label: &str, run: Action) -> CommandAction {
    CommandAction {
        id: id.to_owned(),
        label: label.to_owned(),
        action: run,
        icon: Icon::System,
        group: ActionGroup::Destructive,
        hotkey: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_and_open_build_standard_actions() {
        let copy = copy("a:copy", "text");
        assert_eq!(copy.label, "Copy");
        assert_eq!(copy.action, Action::Copy("text".into()));
        assert_eq!(copy.group, ActionGroup::Standard);

        let open = open_url("a:url", "https://example.com");
        assert_eq!(open.action, Action::OpenUrl("https://example.com".into()));
    }

    #[test]
    fn reveal_picks_the_platform_shell() {
        let reveal = reveal_path("a:reveal", std::path::Path::new("/tmp/x"));
        let Action::RunShell(shell) = reveal.action else {
            panic!("expected a shell action");
        };
        if cfg!(target_os = "macos") {
            assert!(shell.starts_with("open -R /tmp/x"));
        } else if cfg!(target_os = "windows") {
            assert!(shell.starts_with("explorer.exe"));
        } else {
            assert!(shell.starts_with("xdg-open /tmp"));
        }
    }

    #[test]
    fn destructive_groups_apart() {
        let action = destructive("a:delete", "Delete", Action::ShowToast("noop".into()));
        assert_eq!(action.group, ActionGroup::Destructive);
    }

    #[test]
    fn custom_runs_a_concrete_action() {
        let action = custom(
            "a:ping",
            "Ping host",
            Action::RunProcess {
                program: "ping".into(),
                args: vec!["-c".into(), "1".into(), "example.com".into()],
                title: "Ping".into(),
            },
            Icon::Web,
        );
        assert_eq!(action.label, "Ping host");
        assert_eq!(action.group, ActionGroup::Standard);
    }
}
