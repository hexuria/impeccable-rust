//! The pinned tool versions, read from the embedded `tools.txt` and `Dockerfile`.

use std::fmt;

use crate::embedded;

/// A `major.minor.patch` version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version([u64; 3]);

impl Version {
    /// The first `N.N.N` in `text`, which is how tools print their version.
    pub fn find_in(text: &str) -> Option<Self> {
        let bytes = text.as_bytes();
        (0..bytes.len())
            .filter(|&at| {
                bytes[at].is_ascii_digit() && (at == 0 || !bytes[at - 1].is_ascii_digit())
            })
            .find_map(|at| Self::parse_prefix(&text[at..]))
    }

    /// Parses `N.N.N` at the start of `text`, ignoring whatever follows.
    fn parse_prefix(text: &str) -> Option<Self> {
        let mut rest = text;
        let mut parts = [0_u64; 3];
        for (index, part) in parts.iter_mut().enumerate() {
            if index > 0 {
                rest = rest.strip_prefix('.')?;
            }
            let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
            *part = rest[..digits].parse().ok()?;
            rest = &rest[digits..];
        }
        Some(Self(parts))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [major, minor, patch] = self.0;
        write!(f, "{major}.{minor}.{patch}")
    }
}

/// One tool the toolbox installs: its crate name and pinned version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pin {
    pub name: &'static str,
    pub version: Version,
}

/// Every pin in `tools.txt`, in file order.
pub fn all() -> Vec<Pin> {
    parse(embedded::TOOLS)
}

pub fn find(name: &str) -> Option<Pin> {
    all().into_iter().find(|pin| pin.name == name)
}

fn parse(text: &'static str) -> Vec<Pin> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let mut fields = line.split_whitespace();
            let name = fields.next().expect("a non-empty line has a first field");
            let version = fields
                .next()
                .and_then(Version::find_in)
                .unwrap_or_else(|| panic!("tools.txt pins no version for {name}"));
            Pin { name, version }
        })
        .collect()
}

/// The default of a Dockerfile `ARG`, such as `NIGHTLY` or `TLA_VERSION`.
pub fn dockerfile_arg(name: &str) -> Option<&'static str> {
    let prefix = format!("ARG {name}=");
    embedded::DOCKERFILE
        .lines()
        .find_map(|line| line.strip_prefix(prefix.as_str()))
        .map(str::trim_end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_first_version_in_tool_output() {
        let found = |text| Version::find_in(text).map(|version| version.to_string());
        assert_eq!(
            found("cargo-nextest 0.9.146 (abc 2026-01-01)"),
            Some("0.9.146".into())
        );
        assert_eq!(found("valgrind-3.25.1"), Some("3.25.1".into()));
        assert_eq!(found("rustc 1.99 then 1.99.1"), Some("1.99.1".into()));
        assert_eq!(found("no version here 12"), None);
    }

    #[test]
    fn versions_order_by_number_not_by_text() {
        let version = |text| Version::find_in(text).expect("test input holds a version");
        assert!(version("0.9.146") > version("0.9.99"));
        assert!(version("27.1.0") > version("9.99.99"));
        assert_eq!(version("0.50.0"), version("0.50.0"));
    }

    #[test]
    fn the_embedded_pins_parse() {
        let pins = all();
        assert!(pins.len() >= 10, "tools.txt lists the toolbox: {pins:?}");
        assert!(find("kani-verifier").is_some());
        assert!(find("cargo-nextest").is_some());
        assert!(find("no-such-tool").is_none());
    }

    #[test]
    fn reads_dockerfile_args() {
        assert!(dockerfile_arg("NIGHTLY").is_some_and(|value| value.starts_with("nightly-")));
        assert!(dockerfile_arg("TLA_VERSION").is_some());
        assert_eq!(dockerfile_arg("NO_SUCH_ARG"), None);
    }
}
