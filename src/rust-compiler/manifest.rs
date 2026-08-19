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
        Some(self.dependency_root(first)?.join(rest))
    }

    /// The directory a dependency's files live in, or `None` if no dependency has that name.
    ///
    /// **Split out of `resolve_package_import` rather than written beside it**, because `burxt
    /// where` needs the same answer for a bare package name and a second derivation is the failure
    /// this exists to prevent. star-burxt asked for the lookup precisely so that the layout stays an
    /// implementation detail here instead of becoming a contract it re-derives — and its own
    /// re-derivation, a scan of `.burxt/packages`, could not see a **path** dependency at all, which
    /// puts nothing in that directory. One function, both callers, both source kinds.
    pub fn dependency_root(&self, name: &str) -> Option<PathBuf> {
        let dependency = self.dependencies.get(name)?;
        Some(match &dependency.source {
            Source::Path(dir) => self.root.join(dir),
            // A git dependency is built from the cache the fetch populated. The cache path is
            // derived from the URL and tag rather than stored, so two manifests naming the same
            // tag share one copy and neither has to know the other exists.
            Source::Git { url, tag } => {
                self.root.join(".burxt").join("packages").join(cache_key(url, tag))
            }
        })
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

// ---------------------------------------------------------------------------------------------
// The lockfile — `burxt.lock`. C2.
// ---------------------------------------------------------------------------------------------

/// One dependency, pinned to the commit that was actually built.
#[derive(Debug, Clone, PartialEq)]
pub struct Locked {
    pub name: String,
    pub url: String,
    pub tag: String,
    pub commit: String,
}

pub const LOCK_NAME: &str = "burxt.lock";

/// Read `burxt.lock`, or an empty list when there is none.
///
/// Same grammar as the manifest for the same reason — one statement per line, first word is the
/// key — because a lockfile is read by people far more often than it is written, usually while
/// answering "what changed" during a review.
pub fn read_lock(root: &Path) -> Result<Vec<Locked>, String> {
    let path = root.join(LOCK_NAME);
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read {}: {}", path.display(), e))?;
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let words: Vec<&str> = trimmed.split_whitespace().collect();
        match words.as_slice() {
            ["package", name, url, tag, commit] => out.push(Locked {
                name: name.to_string(),
                url: url.to_string(),
                tag: tag.to_string(),
                commit: commit.to_string(),
            }),
            _ => {
                return Err(format!(
                    "{}:{}: a lockfile line is `package <name> <url> <tag> <commit>`. This file is \
                     written by `burxt fetch` — if it has been edited by hand, delete it and fetch \
                     again.",
                    path.display(),
                    n + 1
                ))
            }
        }
    }
    Ok(out)
}

/// Write `burxt.lock`. Sorted by name so two fetches of the same manifest produce the same bytes,
/// which is what makes the file reviewable in a diff.
pub fn write_lock(root: &Path, mut locked: Vec<Locked>) -> Result<(), String> {
    locked.sort_by(|a, b| a.name.cmp(&b.name));
    let mut text = String::from(
        "# Written by `burxt fetch`. Every dependency, pinned to the exact commit that was\n\
         # resolved — so the next person to fetch gets these bytes even if the tag has moved.\n\
         # Commit this file. Do not edit it by hand.\n",
    );
    for entry in &locked {
        text.push_str(&format!(
            "package  {}  {}  {}  {}\n",
            entry.name, entry.url, entry.tag, entry.commit
        ));
    }
    std::fs::write(root.join(LOCK_NAME), text)
        .map_err(|e| format!("cannot write {}: {}", root.join(LOCK_NAME).display(), e))
}

/// `burxt fetch` — populate `.burxt/packages/` and write the lockfile. C2.
///
/// **The only place this compiler touches the network, and only when asked.** A build that fetched
/// silently would mean the same command did different things on different days depending on what a
/// remote had done, which is the opposite of every other guarantee here. `build` reads what is on
/// disk and refuses, by name, when something is missing.
///
/// **With a lockfile present, the LOCKED COMMIT is what gets checked out — not the tag.** That is
/// the whole point of the file: a tag is a name somebody else can move, and the second person to
/// fetch a project should get the bytes the first person built. Without a lock, the tag is resolved
/// once and the commit it pointed at is written down.
pub fn fetch(package: &Manifest) -> Result<String, String> {
    use std::process::Command;
    let cache = package.root.join(".burxt").join("packages");
    std::fs::create_dir_all(&cache)
        .map_err(|e| format!("cannot create {}: {}", cache.display(), e))?;
    let existing = read_lock(&package.root)?;
    let mut locked: Vec<Locked> = Vec::new();
    let mut report = String::new();

    for (name, dependency) in &package.dependencies {
        let Source::Git { url, tag } = &dependency.source else {
            // A path dependency is a directory somebody already has. Nothing to fetch and nothing
            // to pin: it has no version, which is the trade a path dependency makes.
            continue;
        };
        let into = cache.join(cache_key(url, tag));
        let pinned = existing
            .iter()
            .find(|l| l.name == *name && l.url == *url && l.tag == *tag)
            .map(|l| l.commit.clone());

        if !into.join(".git").is_dir() {
            let _ = std::fs::remove_dir_all(&into);
            let out = Command::new("git")
                .args(["clone", "--quiet", url])
                .arg(&into)
                .output()
                .map_err(|e| format!("cannot run git: {}", e))?;
            if !out.status.success() {
                return Err(format!(
                    "could not clone `{}` from {}:\n{}",
                    name,
                    url,
                    String::from_utf8_lossy(&out.stderr).trim()
                ));
            }
        }

        // What to check out: the locked commit if there is one, otherwise the tag.
        let wanted = pinned.clone().unwrap_or_else(|| tag.clone());
        let out = Command::new("git")
            .args(["-C"])
            .arg(&into)
            .args(["checkout", "--quiet", &wanted])
            .output()
            .map_err(|e| format!("cannot run git: {}", e))?;
        if !out.status.success() {
            // A locked commit that is gone is a different failure from a tag that never existed,
            // and the two need different advice.
            return Err(if pinned.is_some() {
                format!(
                    "`{}` is locked to commit {}, and that commit is not in {}. The history was \
                     rewritten or the repository moved. Delete `{}` and fetch again to record what \
                     is there now — and know that you are changing what this project builds.",
                    name, wanted, url, LOCK_NAME
                )
            } else {
                format!(
                    "`{}` has no tag `{}` at {}:\n{}",
                    name,
                    tag,
                    url,
                    String::from_utf8_lossy(&out.stderr).trim()
                )
            });
        }

        let head = Command::new("git")
            .args(["-C"])
            .arg(&into)
            .args(["rev-parse", "HEAD"])
            .output()
            .map_err(|e| format!("cannot run git: {}", e))?;
        let commit = String::from_utf8_lossy(&head.stdout).trim().to_string();
        report.push_str(&format!(
            "{}  {}  {}  {}\n",
            if pinned.is_some() { "locked " } else { "fetched" },
            name,
            tag,
            &commit[..commit.len().min(12)]
        ));
        locked.push(Locked {
            name: name.clone(),
            url: url.clone(),
            tag: tag.clone(),
            commit,
        });
    }

    write_lock(&package.root, locked)?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **`resolve_package_import` must BE `dependency_root` plus the rest, not a second derivation.**
    ///
    /// The layout of a fetched package is deliberately not a contract — `cache_key` is derived rather
    /// than stored — so the moment two places compute it, one of them is wrong on the day it changes.
    /// This pins them to the same function, and pins the property that made the lookup necessary: a
    /// **path** dependency puts nothing under `.burxt/packages`, so a directory scan cannot find one.
    #[test]
    fn a_dependency_root_is_one_derivation_for_both_source_kinds() {
        let text = "name app\n\
                    version 1.0.0\n\
                    dependency vendored ../vendor/mylib\n\
                    dependency remote https://github.com/x/y burxt-1.2.3\n";
        let m = Manifest::parse(text, Path::new("/proj"), "burxt.package").unwrap();

        assert_eq!(m.dependency_root("vendored").unwrap(), Path::new("/proj/../vendor/mylib"));
        assert_eq!(
            m.dependency_root("remote").unwrap(),
            Path::new("/proj/.burxt/packages")
                .join(cache_key("https://github.com/x/y", "burxt-1.2.3"))
        );
        assert!(m.dependency_root("absent").is_none(), "an undeclared name resolves to nothing");

        for name in ["vendored", "remote"] {
            assert_eq!(
                m.resolve_package_import(&format!("{}/a/b.bx", name)).unwrap(),
                m.dependency_root(name).unwrap().join("a/b.bx"),
                "`{}` resolved by a different route than dependency_root",
                name
            );
        }

        assert!(
            !m.dependency_root("vendored").unwrap().starts_with("/proj/.burxt"),
            "a path dependency must not be looked for in the fetch cache — nothing puts it there, \
             which is exactly why a scan of that directory cannot answer this question"
        );
    }
}
