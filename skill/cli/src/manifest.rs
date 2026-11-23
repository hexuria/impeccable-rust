//! Reading the few facts bench-guard needs out of Cargo files and diffs.
//!
//! These are line readers, not a TOML parser: the crate has no dependencies. Each one
//! says what it does not understand.

use std::collections::{BTreeMap, BTreeSet};

/// The lines of a Cargo manifest that configure build profiles, without comments or
/// blank lines, so two manifests compare equal when their profiles are the same.
///
/// It reads `[profile]` and `[profile.*]` tables and top-level `profile.x = ...` keys.
/// A multi-line value that has a line looking like a table header ends the table early.
pub fn profile_settings(manifest: &str) -> Vec<String> {
    let mut settings = Vec::new();
    // `None` until the first table header: the manifest's top level.
    let mut table: Option<&str> = None;
    for line in manifest.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(name) = table_header(line) {
            table = Some(name);
            if is_profile_key(name) {
                settings.push(format!("[{name}]"));
            }
            continue;
        }
        let in_profile = match table {
            Some(name) => is_profile_key(name),
            None => is_profile_key(line.split('=').next().unwrap_or(line).trim()),
        };
        if in_profile {
            settings.push(line.to_owned());
        }
    }
    settings
}

/// The name inside a `[table]` or `[[array-of-tables]]` header line.
fn table_header(line: &str) -> Option<&str> {
    let line = line.split('#').next().unwrap_or(line).trim_end();
    let name = line
        .strip_prefix("[[")
        .and_then(|rest| rest.strip_suffix("]]"))
        .or_else(|| {
            line.strip_prefix('[')
                .and_then(|rest| rest.strip_suffix(']'))
        })?
        .trim();
    // A line of a multi-line array, such as `["-C", "opt-level=3"]`, is not a header.
    let is_key = !name.is_empty() && !name.contains([',', '=', '[', ']']);
    is_key.then_some(name)
}

fn is_profile_key(key: &str) -> bool {
    key == "profile" || key.starts_with("profile.")
}

/// `name version` for every package in a `Cargo.lock`.
pub fn lock_packages(lock: &str) -> BTreeSet<String> {
    let mut packages = BTreeSet::new();
    let mut name = "";
    for line in lock.lines() {
        if let Some(value) = line.strip_prefix("name = ") {
            name = value.trim().trim_matches('"');
        } else if let Some(value) = line.strip_prefix("version = ")
            // The lockfile's own format `version` comes before any package name.
            && !name.is_empty()
        {
            packages.insert(format!("{name} {}", value.trim().trim_matches('"')));
            name = "";
        }
    }
    packages
}

/// For each file in a unified diff, how many added lines contain the word `unsafe`.
pub fn added_unsafe(diff: &str) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    let mut file = "";
    for line in diff.lines() {
        if let Some(path) = line.strip_prefix("+++ ") {
            file = path.strip_prefix("b/").unwrap_or(path);
        } else if let Some(added) = line.strip_prefix('+')
            && has_word(added, "unsafe")
        {
            *counts.entry(file.to_owned()).or_insert(0) += 1;
        }
    }
    counts
}

/// How many lines of `text` contain the word `unsafe`.
pub fn unsafe_lines(text: &str) -> usize {
    text.lines().filter(|line| has_word(line, "unsafe")).count()
}

/// True when `word` appears in `line` with no identifier character on either side.
fn has_word(line: &str, word: &str) -> bool {
    let is_ident = |c: char| c.is_ascii_alphanumeric() || c == '_';
    line.match_indices(word).any(|(at, _)| {
        let before = line[..at].chars().next_back();
        let after = line[at + word.len()..].chars().next();
        !before.is_some_and(is_ident) && !after.is_some_and(is_ident)
    })
}

/// The value of the first `"key":"..."` string field in compact JSON.
pub fn json_string_field(json: &str, key: &str) -> Option<String> {
    let marker = format!("\"{key}\":\"");
    let start = json.find(&marker)? + marker.len();
    let mut value = String::new();
    let mut chars = json[start..].chars();
    loop {
        match chars.next()? {
            '"' => return Some(value),
            '\\' => match chars.next()? {
                'n' => value.push('\n'),
                't' => value.push('\t'),
                'r' => value.push('\r'),
                'u' => {
                    let hex: String = chars.by_ref().take(4).collect();
                    value.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
                }
                // `\\`, `\"`, and `\/` stand for the character itself.
                escaped => value.push(escaped),
            },
            plain => value.push(plain),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_tables_are_extracted_without_comments() {
        let manifest = "\
[package]
name = \"demo\"

# tuned for benchmarks
[profile.release]
lto = true   # fat

[profile.release.package.\"*\"]
opt-level = 3

[dependencies]
serde = \"1\"
";
        assert_eq!(
            profile_settings(manifest),
            [
                "[profile.release]",
                "lto = true   # fat",
                "[profile.release.package.\"*\"]",
                "opt-level = 3",
            ]
        );
    }

    #[test]
    fn every_way_of_writing_a_profile_is_seen() {
        let dotted = "profile.release.lto = true\n[package]\nname = \"demo\"\n";
        assert_eq!(profile_settings(dotted), ["profile.release.lto = true"]);

        let parent = "[profile]\nrelease = { debug = true }\n[lib]\nbench = false\n";
        assert_eq!(
            profile_settings(parent),
            ["[profile]", "release = { debug = true }"]
        );

        let none = "[package]\nname = \"profile\"\nprofiles = 1\n";
        assert!(
            profile_settings(none).is_empty(),
            "{:?}",
            profile_settings(none)
        );
    }

    #[test]
    fn comment_and_spacing_changes_do_not_change_profiles() {
        let before = "[profile.bench]\ndebug = true\n";
        let after = "# why\n\n  [profile.bench]\n\n  debug = true\n";
        assert_eq!(profile_settings(before), profile_settings(after));
        assert_ne!(
            profile_settings(before),
            profile_settings("[profile.bench]\ndebug = false\n")
        );
    }

    #[test]
    fn an_array_line_is_not_a_table_header() {
        assert_eq!(table_header("[profile.release]"), Some("profile.release"));
        assert_eq!(table_header("[[bench]]  # harness"), Some("bench"));
        assert_eq!(table_header("[\"-C\", \"target-cpu=native\"],"), None);
        assert_eq!(table_header("lto = true"), None);
    }

    #[test]
    fn lock_packages_pair_names_with_versions() {
        let lock = "\
version = 4

[[package]]
name = \"alpha\"
version = \"1.0.0\"
dependencies = [
 \"beta\",
]

[[package]]
name = \"beta\"
version = \"0.2.1\"
";
        let packages: Vec<String> = lock_packages(lock).into_iter().collect();
        assert_eq!(packages, ["alpha 1.0.0", "beta 0.2.1"]);
    }

    #[test]
    fn added_unsafe_counts_only_added_lines_with_the_word() {
        let diff = "\
diff --git a/src/a.rs b/src/a.rs
--- a/src/a.rs
+++ b/src/a.rs
@@ -1,0 +2,3 @@
+    unsafe { ptr.read() }
+    let not_unsafe_here = unsafety();
+unsafe impl Send for A {}
-    unsafe { removed() }
diff --git a/src/b.rs b/src/b.rs
--- a/src/b.rs
+++ b/src/b.rs
@@ -1,0 +2 @@
+fn safe() {}
";
        let counts = added_unsafe(diff);
        assert_eq!(counts.get("src/a.rs"), Some(&2));
        assert_eq!(counts.get("src/b.rs"), None);
        assert_eq!(counts.len(), 1);
    }

    #[test]
    fn unsafe_is_matched_as_a_whole_word() {
        assert!(has_word("unsafe fn f()", "unsafe"));
        assert!(has_word("pub(crate) unsafe{", "unsafe"));
        assert!(!has_word("#![forbid(unsafe_code)]", "unsafe"));
        assert!(!has_word("let munsafe = 1;", "unsafe"));
        assert_eq!(unsafe_lines("unsafe {}\nsafe\n// unsafe: no\n"), 2);
    }

    #[test]
    fn json_strings_are_unescaped() {
        let json =
            r#"{"packages":[],"target_directory":"C:\\work\\my \"crate\"\\target","version":1}"#;
        assert_eq!(
            json_string_field(json, "target_directory").as_deref(),
            Some(r#"C:\work\my "crate"\target"#)
        );
        assert_eq!(
            json_string_field(
                r#"{"target_directory":"/tmp/caf\u00e9"}"#,
                "target_directory"
            )
            .as_deref(),
            Some("/tmp/café")
        );
        assert_eq!(json_string_field(json, "missing"), None);
    }
}
