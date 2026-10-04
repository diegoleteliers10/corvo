//! The Manifest module: what an extension declares about itself —
//! identity (title, description, icon) and its command surface with
//! typed arguments. Root search and Settings render from the
//! manifest, so declared commands show up without hand-written
//! plumbing.

use corvo_core::{
    ArgumentKind, ArgumentSpec, CommandMode, CommandSpec, ExtensionManifest, Icon,
};

/// Builds the extension manifest. `commands` is every declared
/// command, in display order (required arguments first).
pub fn extension(
    name: &'static str,
    title: &'static str,
    description: &'static str,
    icon: Icon,
    categories: &'static [&'static str],
    commands: Vec<CommandSpec>,
) -> ExtensionManifest {
    ExtensionManifest {
        name,
        title,
        description,
        icon,
        categories,
        commands,
    }
}

/// A `View` command: it opens a page and the user interacts with it.
pub fn view_command(
    name: &'static str,
    title: &'static str,
    description: &'static str,
    keywords: &'static [&'static str],
) -> CommandSpec {
    CommandSpec {
        name,
        title,
        description,
        mode: CommandMode::View,
        icon: None,
        arguments: Vec::new(),
        keywords,
    }
}

/// A `NoView` command: it acts immediately and confirms with a toast.
pub fn action_command(
    name: &'static str,
    title: &'static str,
    description: &'static str,
    keywords: &'static [&'static str],
) -> CommandSpec {
    CommandSpec {
        name,
        title,
        description,
        mode: CommandMode::NoView,
        icon: None,
        arguments: Vec::new(),
        keywords,
    }
}

/// Fluent additions to a `CommandSpec`, so builders can chain
/// `.icon(...)` and `.argument(...)`.
pub trait CommandSpecBuilder {
    fn icon(self, icon: Icon) -> Self;
    fn argument(self, argument: ArgumentSpec) -> Self;
}

impl CommandSpecBuilder for CommandSpec {
    /// Overrides the extension icon for this command.
    fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Appends a declared argument. Order matters: required first.
    fn argument(mut self, argument: ArgumentSpec) -> Self {
        self.arguments.push(argument);
        self
    }
}

/// A required text argument, e.g. `countdown 2026-12-25`.
pub fn text_argument(name: &'static str, placeholder: &'static str) -> ArgumentSpec {
    ArgumentSpec {
        name,
        placeholder,
        kind: ArgumentKind::Text,
        required: false,
    }
}

/// `required: true` — the launcher keeps asking until it is filled.
pub fn required_text_argument(name: &'static str, placeholder: &'static str) -> ArgumentSpec {
    ArgumentSpec {
        name,
        placeholder,
        kind: ArgumentKind::Text,
        required: true,
    }
}

/// A masked text argument.
pub fn password_argument(name: &'static str, placeholder: &'static str) -> ArgumentSpec {
    ArgumentSpec {
        name,
        placeholder,
        kind: ArgumentKind::Password,
        required: false,
    }
}

/// A fixed-choice argument; `options` is `(value, title)` pairs.
pub fn dropdown_argument(
    name: &'static str,
    placeholder: &'static str,
    options: Vec<(String, String)>,
) -> ArgumentSpec {
    ArgumentSpec {
        name,
        placeholder,
        kind: ArgumentKind::Dropdown(options),
        required: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_a_manifest_with_arguments() {
        let manifest = extension(
            "countdown",
            "Countdown",
            "Days until a date",
            Icon::Svg("countdown-glyph"),
            &["Productivity"],
            vec![
                view_command("view", "Countdown", "Open the countdown page", &["countdown"])
                    .argument(required_text_argument("date", "YYYY-MM-DD")),
                action_command("today", "Countdown Today", "Copy the date in 30 days", &[]),
            ],
        );
        assert_eq!(manifest.commands.len(), 2);
        assert_eq!(manifest.commands[0].mode, CommandMode::View);
        assert_eq!(manifest.commands[0].arguments.len(), 1);
        assert_eq!(manifest.commands[0].arguments[0].name, "date");
        assert_eq!(manifest.commands[1].mode, CommandMode::NoView);
    }
}
