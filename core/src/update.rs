//! Is there a newer ctail? Asks GitHub's releases API and compares versions.
//!
//! Mirrors `app.go`'s `fetchLatestRelease`, and lives here rather than in a
//! front end because every front end asks the same question of the same URL
//! and reads the answer the same way. The HTTP call blocks; a front end runs
//! it on a thread of its own and takes the answer back however it likes.
//! App Store builds should leave the check disabled (the store handles
//! updates) through the `disable_update_check` setting.

use crate::net;
use std::time::Duration;

/// Where the latest release is described.
pub const RELEASES_API: &str = "https://api.github.com/repos/bisand/ctail/releases/latest";

/// Where to send a reader when nothing better is known.
const RELEASES_PAGE: &str = "https://github.com/bisand/ctail/releases";

/// The outcome of a check. `error` set means nothing else is: the check did
/// not get an answer, and says why in a sentence fit for a dialog.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct UpdateCheck {
    pub current: String,
    pub latest: String,
    pub update_available: bool,
    /// The release notes, as written on GitHub.
    pub notes: String,
    /// The release page.
    pub url: String,
    pub error: Option<String>,
}

impl UpdateCheck {
    fn failed(current: &str, error: String) -> Self {
        Self {
            current: current.to_string(),
            error: Some(error),
            ..Self::default()
        }
    }
}

/// Asks GitHub whether a version newer than `current` has been released.
pub fn check_for_update(current: &str) -> UpdateCheck {
    check_for_update_at(RELEASES_API, current)
}

/// [`check_for_update`] against any URL that answers like the releases API.
pub fn check_for_update_at(api: &str, current: &str) -> UpdateCheck {
    let reply = match net::get(
        api,
        &[("Accept", "application/vnd.github+json")],
        Duration::from_secs(15),
    ) {
        Ok(reply) => reply,
        Err(e) => return UpdateCheck::failed(current, format!("Failed to check for updates: {e}")),
    };
    if reply.status != 200 {
        return UpdateCheck::failed(
            current,
            format!("Failed to check for updates (HTTP {})", reply.status),
        );
    }
    parse_release(current, &reply.body)
}

/// Reads a releases-API answer. Pure, so the shape GitHub sends can be
/// tested without GitHub.
pub fn parse_release(current: &str, json: &str) -> UpdateCheck {
    let Ok(release) = serde_json::from_str::<serde_json::Value>(json) else {
        return UpdateCheck::failed(current, "Failed to parse update info".into());
    };
    let Some(tag) = release.get("tag_name").and_then(|t| t.as_str()) else {
        return UpdateCheck::failed(current, "Failed to parse update info".into());
    };
    let latest = tag.strip_prefix('v').unwrap_or(tag).to_string();
    let notes = release
        .get("body")
        .and_then(|b| b.as_str())
        .unwrap_or("")
        .to_string();
    let url = release
        .get("html_url")
        .and_then(|u| u.as_str())
        .unwrap_or(RELEASES_PAGE)
        .to_string();
    UpdateCheck {
        current: current.to_string(),
        update_available: compare_versions(&latest, current) > 0,
        latest,
        notes,
        url,
        error: None,
    }
}

/// Positive when `a` is newer than `b`, negative when older, zero when the
/// same. Dotted numeric components compared numerically — `1.10` is newer than
/// `1.9` — with missing components read as 0 and anything after a `+` (a
/// build number) ignored.
pub fn compare_versions(a: &str, b: &str) -> i32 {
    fn parts(v: &str) -> Vec<u64> {
        v.split('+')
            .next()
            .unwrap_or("")
            .split('.')
            .map(|p| {
                p.chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .parse()
                    .unwrap_or(0)
            })
            .collect()
    }
    let (pa, pb) = (parts(a), parts(b));
    for i in 0..pa.len().max(pb.len()) {
        let x = pa.get(i).copied().unwrap_or(0);
        let y = pb.get(i).copied().unwrap_or(0);
        if x != y {
            return if x < y { -1 } else { 1 };
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_numerically_by_component() {
        assert!(compare_versions("1.0.1", "1.0.0") > 0, "patch newer");
        assert!(compare_versions("1.0.0", "1.0.1") < 0, "patch older");
        assert!(
            compare_versions("1.2.0", "1.10.0") < 0,
            "numeric, not lexical"
        );
        assert_eq!(compare_versions("0.9.9", "0.9.9"), 0, "equal");
        assert_eq!(
            compare_versions("1.0", "1.0.0"),
            0,
            "missing components are 0"
        );
        assert!(compare_versions("2.0.0", "1.9.9") > 0, "major newer");
        assert_eq!(
            compare_versions("0.9.9+255", "0.9.9"),
            0,
            "build suffix ignored"
        );
        assert_eq!(
            compare_versions("1.2.3-beta", "1.2.3"),
            0,
            "a suffix on a component is dropped"
        );
    }

    #[test]
    fn a_release_is_read_the_way_github_writes_it() {
        let json = r#"{"tag_name":"v1.4.0","body":"Fixes.","html_url":"https://github.com/bisand/ctail/releases/tag/v1.4.0"}"#;
        let check = parse_release("1.3.2", json);
        assert_eq!(check.latest, "1.4.0");
        assert!(check.update_available);
        assert_eq!(check.notes, "Fixes.");
        assert_eq!(
            check.url,
            "https://github.com/bisand/ctail/releases/tag/v1.4.0"
        );
        assert_eq!(check.error, None);

        let same = parse_release("1.4.0", json);
        assert!(!same.update_available, "not newer than itself");

        let bare = parse_release("1.0.0", r#"{"tag_name":"2.0.0"}"#);
        assert_eq!(bare.latest, "2.0.0", "a tag without a v is fine");
        assert_eq!(bare.url, RELEASES_PAGE, "no page named: the releases list");

        let broken = parse_release("1.0.0", "not json");
        assert_eq!(broken.error.as_deref(), Some("Failed to parse update info"));
        assert!(!broken.update_available);
        let no_tag = parse_release("1.0.0", r#"{"body":"x"}"#);
        assert!(no_tag.error.is_some());
    }

    #[test]
    fn the_check_goes_over_http_and_reads_the_status() {
        let (url, server) =
            net::testing::serve_once(200, r#"{"tag_name":"v9.9.9","body":"","html_url":"u"}"#);
        let check = check_for_update_at(&url, "1.0.0");
        assert_eq!(check.error, None, "{check:?}");
        assert!(check.update_available);
        assert_eq!(check.latest, "9.9.9");
        let request = server.join().unwrap();
        assert!(request.starts_with("GET / HTTP/1.1"), "{request}");
        assert!(
            request
                .to_ascii_lowercase()
                .contains("accept: application/vnd.github+json"),
            "asks for the API's own media type: {request}"
        );

        let (url, server) = net::testing::serve_once(503, "down");
        let check = check_for_update_at(&url, "1.0.0");
        assert_eq!(
            check.error.as_deref(),
            Some("Failed to check for updates (HTTP 503)")
        );
        server.join().unwrap();

        let unreachable = check_for_update_at("http://127.0.0.1:1/", "1.0.0");
        assert!(
            unreachable
                .error
                .as_deref()
                .is_some_and(|e| e.starts_with("Failed to check for updates: ")),
            "{unreachable:?}"
        );
    }
}
