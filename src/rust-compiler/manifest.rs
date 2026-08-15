//! The package manifest — `burxt.package`. Roadmap C2.
//!
//! # Why the format is boring on purpose
//!
//! Not TOML: there is no TOML parser here and adding one to read four keys is a dependency and a
//! grammar for the sake of punctuation. Not a Burxt program either, which was the tempting answer
//! for a self-hosting language — **a manifest that can compute is a manifest a reviewer has to
//! execute in their head**, and this project's whole case is that a reviewer can see what a change
//! did by reading it. Gradle is what happens when the build file becomes a language.
//!
//! So: one statement per line, the first word is the key, the rest are its words. No nesting, no
//! quoting, no expressions, no conditionals. A person can read the whole grammar in one sentence
//! and diff two of them by eye.
//!
//! ```text
//! name        ledger
//! version     0.1.0
//! dependency  money  ./vendor/money
//! dependency  http   https://github.com/someone/http.bx  v1.2.0
//! ```
//!
//! # What a dependency resolves to
//!
//! A `use "money/decimal.bx"` whose first segment names a declared dependency is a PACKAGE import
//! and resolves under that dependency's root. Everything else stays exactly what it was: a path
//! relative to the file doing the importing, which is what every `use` in this repository is today.
//!
//! **An import that could be read both ways is refused rather than resolved.** If `money` is a
//! declared dependency and a directory called `money` also sits beside the importing file, the
//! compiler stops and says so. Picking one silently — or worse, picking whichever happens to exist
//! — makes resolution depend on the shape of a directory tree, and the failure would appear on
//! someone else's machine.
//!
//! # What is NOT here, deliberately
//!
//! No registry, per the roadmap: a registry is an operational commitment rather than a language
//! feature, and one is not needed to depend on somebody's code. No version RANGES either — a
//! dependency names one tag, the lockfile pins one commit, and resolution is therefore a lookup
//! rather than a solver. Ranges are what make dependency resolution a research problem, and the
//! thing they buy — automatic minor upgrades — is the thing `burxt review` is meant to make a
//! decision rather than a default.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Where a dependency's source comes from.
#[derive(Debug, Clone, PartialEq)]
pub enum Source {
    /// A directory on this machine. Vendored code, a sibling checkout, a workspace member.
    Path(String),
    /// A git repository at a tag. The tag is what a person wrote; the LOCKFILE holds the commit
    /// that tag pointed at, which is what actually gets built.
    Git { url: String, tag: String },
}

/// `name` is unread until the lockfile writes it out — kept because a dependency without its own
/// name in the record is a row that cannot be diffed.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct Dependency {
    pub name: String,
    pub source: Source,
}

/// `name` and `version` are unread until `burxt review` becomes the semver rule (C2's last slice)
/// and the lockfile records what was built. Parsed and checked now so a manifest missing either is
/// refused on the day it is written rather than on the day it is published.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct Manifest {
    /// The directory the manifest was found in. Every relative path in it is relative to THIS,
    /// not to the file being compiled — otherwise the same manifest would mean different things
    /// depending on which source file you happened to name on the command line.
    pub root: PathBuf,
    pub name: String,
    pub version: String,
    pub dependencies: BTreeMap<String, Dependency>,
}

pub const MANIFEST_NAME: &str = "burxt.package";

impl Manifest {
    /// Look for `burxt.package` beside `start`, then in each parent directory.
    ///
    /// Walking up is what lets `burxt build src/main.bx` work from the project root, which is how
    /// everyone will type it. Answering `None` is not an error: a single-file program with no
    /// dependencies has nothing to declare, and requiring a manifest to compile one would make the
    /// language harder to try than it needs to be.
    pub fn discover(start: &Path) -> Result<Option<Manifest>, String> {
        let mut dir = if start.is_dir() {
            Some(start.to_path_buf())
        } else {
            start.parent().map(|p| p.to_path_buf())
        };
        while let Some(here) = dir {
            let candidate = here.join(MANIFEST_NAME);
            if candidate.is_file() {
                let text = std::fs::read_to_string(&candidate)
                    .map_err(|e| format!("cannot read {}: {}", candidate.display(), e))?;
                return Manifest::parse(&text, &here, &candidate.display().to_string()).map(Some);
            }
            dir = here.parent().map(|p| p.to_path_buf());
        }
        Ok(None)
    }

    /// Parse a manifest. Every refusal names the line, because a manifest is edited by hand.
    pub fn parse(text: &str, root: &Path, shown_as: &str) -> Result<Manifest, String> {
        let mut name = None;
        let mut version = None;
        let mut dependencies: BTreeMap<String, Dependency> = BTreeMap::new();

        for (n, line) in text.lines().enumerate() {
            let at = n + 1;
            let trimmed = line.trim();
            // `#` and not `//`, because this is not Burxt source and reading it as though it were
            // is how someone ends up expecting interpolation in it.
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let mut words = trimmed.split_whitespace();
            let key = words.next().unwrap_or("");
            let rest: Vec<&str> = words.collect();
            match key {
                "name" | "version" => {
                    if rest.len() != 1 {
                        return Err(format!(
                            "{}:{}: `{}` takes exactly one word, and this line has {}",
                            shown_as,
                            at,
                            key,
                            rest.len()
                        ));
                    }
                    let slot = if key == "name" { &mut name } else { &mut version };
                    if slot.is_some() {
                        return Err(format!(
                            "{}:{}: `{}` is declared twice. One manifest describes one package.",
                            shown_as, at, key
                        ));
                    }
                    *slot = Some(rest[0].to_string());
                }
                "dependency" => {
                    // `dependency <name> <path>` or `dependency <name> <url> <tag>`.
                    let (dep, source) = match rest.as_slice() {
                        [dep, where_from] => (*dep, Source::Path(where_from.to_string())),
                        [dep, url, tag] => (
                            *dep,
                            Source::Git { url: url.to_string(), tag: tag.to_string() },
                        ),
                        _ => {
                            return Err(format!(
                                "{}:{}: a dependency is `dependency <name> <directory>` or \
                                 `dependency <name> <git-url> <tag>`",
                                shown_as, at
                            ))
                        }
                    };
                    if !is_plain_name(dep) {
                        return Err(format!(
                            "{}:{}: `{}` is not a usable dependency name — a name is letters, \
                             digits and `_`, because it becomes the first segment of a `use` path",
                            shown_as, at, dep
                        ));
                    }
                    // A git source must name a tag. Tracking a branch means the same manifest
                    // builds different code on different days, which is the property a lockfile
                    // exists to remove — so it is refused where it is written rather than
                    // discovered when a build stops reproducing.
                    if let Source::Git { url, .. } = &source {
                        if !url.contains("://") {
                            return Err(format!(
                                "{}:{}: `{}` has three words, so the second is read as a git URL, \
                                 and `{}` is not one. A directory takes two words.",
                                shown_as, at, key, url
                            ));
                        }
                    }
                    if dependencies.contains_key(dep) {
                        return Err(format!(
                            "{}:{}: `{}` is declared twice, and the two lines cannot both be the \
                             one that is used",
                            shown_as, at, dep
                        ));
                    }
                    dependencies
                        .insert(dep.to_string(), Dependency { name: dep.to_string(), source });
                }
                other => {
                    return Err(format!(
                        "{}:{}: unknown key `{}`. A manifest has `name`, `version` and \
                         `dependency` — and that is the whole grammar.",
                        shown_as, at, other
                    ))
                }
            }
        }

        Ok(Manifest {
            root: root.to_path_buf(),
            name: name.ok_or_else(|| format!("{}: no `name` line", shown_as))?,
            version: version.ok_or_else(|| format!("{}: no `version` line", shown_as))?,
            dependencies,
        })
    }

    /// Where a package import lands, or `None` if the first segment names no dependency.
    ///
    /// `money/decimal.bx` under `dependency money ./vendor/money` is `./vendor/money/decimal.bx`,
    /// relative to the MANIFEST's directory rather than to the importing file.
    pub fn resolve_package_import(&self, import: &str) -> Option<PathBuf> {
        let (first, rest) = import.split_once('/')?;
        let dependency = self.dependencies.get(first)?;
        match &dependency.source {
            Source::Path(dir) => Some(self.root.join(dir).join(rest)),
            // A git dependency is built from the cache the fetch populated. The cache path is
            // derived from the URL and tag rather than stored, so two manifests naming the same
            // tag share one copy and neither has to know the other exists.
            Source::Git { url, tag } => {
                Some(self.root.join(".burxt").join("packages").join(cache_key(url, tag)).join(rest))
            }
        }
    }
}

/// A directory name for a git dependency, derived from what identifies it.
///
/// Readable rather than hashed: someone looking in `.burxt/packages` should be able to tell what
/// they are looking at without a lookup table. Anything that is not a name character becomes `-`,
/// which can collide in principle — two URLs differing only in punctuation — and the tag being part
/// of the key makes that vanishingly unlikely in the case that matters, two versions of one package.
pub fn cache_key(url: &str, tag: &str) -> String {
    let squash = |s: &str| -> String {
        s.chars().map(|c| if c.is_alphanumeric() || c == '_' { c } else { '-' }).collect()
    };
    format!("{}-{}", squash(url.trim_end_matches('/')), squash(tag))
}

fn is_plain_name(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '_')
}
