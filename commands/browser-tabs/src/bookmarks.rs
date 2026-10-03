//! Bookmark loading per browser: Chromium profiles answer a `Bookmarks`
//! JSON tree; Safari answers `Bookmarks.plist` converted through
//! `plutil`. Both paths degrade to an empty list when the file is
//! unreadable (Safari may need Full Disk Access).

use super::Bookmark;
use super::BrowserId;

/// Reads this browser's bookmarks.
pub fn load(id: BrowserId) -> Vec<Bookmark> {
    let spec = id.spec();
    if spec.safari_bookmarks {
        return safari_bookmarks();
    }
    chromium_bookmarks(spec.chromium_dirs)
}

/// Walks the browser's Chromium profile tree.
fn chromium_bookmarks(dirs: Option<(&'static str, &'static str, &'static str)>) -> Vec<Bookmark> {
    let Some((macos_relative, windows_relative, linux_relative)) = dirs else {
        return Vec::new();
    };
    for base in corvo_platform::browser_data_candidates(
        macos_relative,
        windows_relative,
        linux_relative,
    ) {
        let path = base.join("Default").join("Bookmarks");
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
            let mut bookmarks = Vec::new();
            for root in ["bookmark_bar", "other", "synced"] {
                if let Some(node) = value.pointer(&format!("/roots/{root}")) {
                    walk_chromium(node, &mut bookmarks);
                }
            }
            return bookmarks;
        }
    }
    Vec::new()
}

fn walk_chromium(node: &serde_json::Value, out: &mut Vec<Bookmark>) {
    let Some(children) = node
        .get("children")
        .and_then(|children| children.as_array())
    else {
        return;
    };
    for child in children {
        if let Some(url) = child.get("url").and_then(|url| url.as_str()) {
            out.push(Bookmark {
                title: child
                    .get("name")
                    .and_then(|name| name.as_str())
                    .unwrap_or(url)
                    .to_owned(),
                url: url.to_owned(),
            });
        }
        walk_chromium(child, out);
    }
}

/// Reads Safari's `Bookmarks.plist` through `plutil -convert json`.
/// Reading `~/Library/Safari` can require Full Disk Access; failure
/// simply yields no bookmarks.
fn safari_bookmarks() -> Vec<Bookmark> {
    let Some(home) = std::env::var_os("HOME") else {
        return Vec::new();
    };
    let plist = std::path::PathBuf::from(home)
        .join("Library")
        .join("Safari")
        .join("Bookmarks.plist");
    if !plist.is_file() {
        return Vec::new();
    }
    let Ok(output) = std::process::Command::new("plutil")
        .arg("-convert")
        .arg("json")
        .arg("-o")
        .arg("-")
        .arg(&plist)
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&output.stdout) else {
        return Vec::new();
    };
    let mut bookmarks = Vec::new();
    walk_safari(&value, &mut bookmarks);
    bookmarks
}

fn walk_safari(node: &serde_json::Value, out: &mut Vec<Bookmark>) {
    if let Some(url) = node.get("URLString").and_then(|url| url.as_str()) {
        let title = node
            .pointer("/URIDictionary/title")
            .and_then(|title| title.as_str())
            .unwrap_or(url);
        out.push(Bookmark {
            title: title.to_owned(),
            url: url.to_owned(),
        });
    }
    if let Some(children) = node.get("Children").and_then(|list| list.as_array()) {
        for child in children {
            walk_safari(child, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walks_nested_chromium_trees() {
        let tree: serde_json::Value = serde_json::from_str(
            r#"{"children": [
                {"name": "Rust", "url": "https://rust-lang.org"},
                {"name": "Docs", "children": [
                    {"name": "std", "url": "https://doc.rust-lang.org/std/"},
                    {"name": "More", "children": [
                        {"name": "cargo", "url": "https://doc.rust-lang.org/cargo/"}
                    ]}
                ]}
            ]}"#,
        )
        .unwrap();
        let mut bookmarks = Vec::new();
        walk_chromium(&tree, &mut bookmarks);
        assert_eq!(bookmarks.len(), 3);
        assert_eq!(bookmarks[0].title, "Rust");
        assert_eq!(bookmarks[2].url, "https://doc.rust-lang.org/cargo/");
    }

    #[test]
    fn walks_safari_trees() {
        // The shape `plutil -convert json` produces from Bookmarks.plist.
        let tree: serde_json::Value = serde_json::from_str(
            r#"{
                "Children": [
                    {"Title": "BookmarksBar", "Children": [
                        {"URIDictionary": {"title": "Rust"},
                         "URLString": "https://rust-lang.org",
                         "WebBookmarkType": "WebBookmarkTypeLeaf"},
                        {"Title": "Folder", "Children": [
                            {"URIDictionary": {"title": "Docs"},
                             "URLString": "https://doc.rust-lang.org/",
                             "WebBookmarkType": "WebBookmarkTypeLeaf"}
                        ]}
                    ]}
                ]
            }"#,
        )
        .unwrap();
        let mut bookmarks = Vec::new();
        walk_safari(&tree, &mut bookmarks);
        assert_eq!(bookmarks.len(), 2);
        assert_eq!(bookmarks[0].title, "Rust");
        assert_eq!(bookmarks[1].url, "https://doc.rust-lang.org/");
    }

    #[test]
    fn chromium_browsers_without_a_profile_answer_empty() {
        assert!(chromium_bookmarks(None).is_empty());
        // Arc and Firefox have no Chromium dirs in the spec table.
        assert!(load(BrowserId::Arc).is_empty());
        assert!(load(BrowserId::Firefox).is_empty());
    }
}
