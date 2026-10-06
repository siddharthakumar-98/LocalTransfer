//! The mock peer's in-memory file tree, seeded from `fixtures/mock-tree.json`.

use std::collections::BTreeMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Deserialize;

use crate::backend::{BackendError, Entry, EntryKind};
use crate::naming::{numbered_name, validate_component};

const FIXTURE: &str = include_str!("../../fixtures/mock-tree.json");

#[derive(Deserialize)]
struct Fixture {
    root: String,
    entries: Vec<FixtureNode>,
}

#[derive(Deserialize)]
struct FixtureNode {
    name: String,
    modified: u64,
    size: Option<u64>,
    children: Option<Vec<FixtureNode>>,
}

#[derive(Debug, Clone)]
pub(crate) enum Node {
    File {
        size: u64,
        modified: SystemTime,
    },
    Dir {
        modified: SystemTime,
        children: BTreeMap<String, Node>,
    },
}

/// A file found under a requested path, relative to that path's parent.
#[derive(Debug, Clone)]
pub(crate) struct FoundFile {
    pub rel: Vec<String>,
    pub size: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct FakeTree {
    pub root_name: String,
    children: BTreeMap<String, Node>,
}

impl FakeTree {
    /// Loads the built-in fixture.
    ///
    /// Panics only if the fixture compiled into the crate is malformed, which
    /// the unit tests rule out.
    pub(crate) fn from_fixture() -> Self {
        let fixture: Fixture =
            serde_json::from_str(FIXTURE).expect("fixtures/mock-tree.json is valid");
        Self {
            root_name: fixture.root,
            children: fixture.entries.into_iter().map(convert).collect(),
        }
    }

    fn dir(&self, components: &[String]) -> Option<&BTreeMap<String, Node>> {
        let mut dir = &self.children;
        for c in components {
            match dir.get(c)? {
                Node::Dir { children, .. } => dir = children,
                Node::File { .. } => return None,
            }
        }
        Some(dir)
    }

    pub(crate) fn get(&self, components: &[String]) -> Option<&Node> {
        let (last, parent) = components.split_last()?;
        self.dir(parent)?.get(last)
    }

    pub(crate) fn is_dir(&self, components: &[String]) -> bool {
        self.dir(components).is_some()
    }

    pub(crate) fn list(&self, components: &[String]) -> Result<Vec<Entry>, BackendError> {
        let dir = self.dir(components).ok_or(BackendError::NotFound)?;
        Ok(dir
            .iter()
            .map(|(name, node)| match node {
                Node::File { size, modified } => Entry {
                    name: name.clone(),
                    kind: EntryKind::File,
                    size: *size,
                    modified: *modified,
                    type_hint: None,
                },
                Node::Dir { modified, .. } => Entry {
                    name: name.clone(),
                    kind: EntryKind::Dir,
                    size: 0,
                    modified: *modified,
                    type_hint: Some("public.folder".to_owned()),
                },
            })
            .collect())
    }

    /// Every file under `components` (a file or a folder), plus every folder,
    /// with paths relative to the parent of `components`.
    pub(crate) fn walk(
        &self,
        components: &[String],
    ) -> Result<(Vec<FoundFile>, Vec<Vec<String>>), BackendError> {
        let node = self.get(components).ok_or(BackendError::NotFound)?;
        let top = components
            .last()
            .cloned()
            .ok_or(BackendError::InvalidPath)?;
        let (mut files, mut dirs) = (Vec::new(), Vec::new());
        collect(node, vec![top], &mut files, &mut dirs);
        Ok((files, dirs))
    }

    /// The first free name for `name` in the folder `parent`: `name`, `name (1)`, …
    /// Returns the name and whether it differs from `name`.
    pub(crate) fn unique_name(&self, parent: &[String], name: &str) -> (String, bool) {
        let Some(dir) = self.dir(parent) else {
            return (name.to_owned(), false);
        };
        let free = first_free(name, |n| dir.contains_key(n));
        let renamed = free != name;
        (free, renamed)
    }

    /// Creates `components` (and any missing parents) as folders.
    pub(crate) fn ensure_dir(&mut self, components: &[String]) -> &mut BTreeMap<String, Node> {
        let mut dir = &mut self.children;
        for c in components {
            // If a file is in the way, keep it and put the folder next to it.
            let name = if matches!(dir.get(c), Some(Node::File { .. })) {
                first_free(c, |n| dir.contains_key(n))
            } else {
                c.clone()
            };
            let node = dir.entry(name).or_insert_with(|| Node::Dir {
                modified: SystemTime::now(),
                children: BTreeMap::new(),
            });
            let Node::Dir { children, .. } = node else {
                unreachable!("first_free never returns a taken name");
            };
            dir = children;
        }
        dir
    }

    /// Adds a file without replacing anything: a clash gets `name (1)`, ….
    /// Returns the final name and whether it was renamed.
    pub(crate) fn insert_file(
        &mut self,
        parent: &[String],
        name: &str,
        size: u64,
        modified: SystemTime,
    ) -> (String, bool) {
        let (final_name, renamed) = self.unique_name(parent, name);
        self.ensure_dir(parent)
            .insert(final_name.clone(), Node::File { size, modified });
        (final_name, renamed)
    }
}

/// `name`, or the first of `name (1)`, `name (2)`, … that isn't taken.
pub(crate) fn first_free(name: &str, taken: impl Fn(&str) -> bool) -> String {
    let mut n = 0;
    loop {
        let candidate = numbered_name(name, n);
        if !taken(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

fn convert(node: FixtureNode) -> (String, Node) {
    debug_assert!(validate_component(&node.name).is_ok(), "{}", node.name);
    let modified = UNIX_EPOCH + Duration::from_secs(node.modified);
    let built = match node.children {
        Some(children) => Node::Dir {
            modified,
            children: children.into_iter().map(convert).collect(),
        },
        None => Node::File {
            size: node.size.unwrap_or(0),
            modified,
        },
    };
    (node.name, built)
}

fn collect(node: &Node, rel: Vec<String>, files: &mut Vec<FoundFile>, dirs: &mut Vec<Vec<String>>) {
    match node {
        Node::File { size, .. } => files.push(FoundFile { rel, size: *size }),
        Node::Dir { children, .. } => {
            dirs.push(rel.clone());
            for (name, child) in children {
                let mut child_rel = rel.clone();
                child_rel.push(name.clone());
                collect(child, child_rel, files, dirs);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn fixture_loads_with_about_200_entries_3_levels_deep() {
        fn count(d: &BTreeMap<String, Node>, depth: usize, max: &mut usize) -> usize {
            *max = (*max).max(depth);
            d.values()
                .map(|n| match n {
                    Node::File { .. } => 1,
                    Node::Dir { children, .. } => 1 + count(children, depth + 1, max),
                })
                .sum()
        }
        let tree = FakeTree::from_fixture();
        assert_eq!(tree.root_name, "desktop");
        let mut depth = 0;
        let total = count(&tree.children, 1, &mut depth);
        assert!((150..=250).contains(&total), "{total}");
        assert_eq!(depth, 3);
    }

    #[test]
    fn listing_and_walking() {
        let tree = FakeTree::from_fixture();
        let root = tree.list(&[]).unwrap();
        assert!(
            root.iter()
                .any(|e| e.name == "Projects" && e.kind == EntryKind::Dir)
        );
        assert!(
            root.iter()
                .any(|e| e.name == "data.unknownext" && e.kind == EntryKind::File)
        );
        assert_eq!(tree.list(&path(&["Nope"])), Err(BackendError::NotFound));
        assert_eq!(tree.list(&path(&["todo.txt"])), Err(BackendError::NotFound));

        let (files, dirs) = tree.walk(&path(&["Projects"])).unwrap();
        assert!(files.iter().all(|f| f.rel[0] == "Projects"));
        assert!(dirs.contains(&path(&["Projects", "Website"])));
    }

    #[test]
    fn inserts_never_replace() {
        let mut tree = FakeTree::from_fixture();
        let now = SystemTime::now();
        assert_eq!(
            tree.insert_file(&[], "todo.txt", 1, now),
            ("todo (1).txt".to_owned(), true)
        );
        assert_eq!(
            tree.insert_file(&[], "todo.txt", 1, now),
            ("todo (2).txt".to_owned(), true)
        );
        assert_eq!(
            tree.insert_file(&path(&["New", "Deep"]), "a.md", 1, now),
            ("a.md".to_owned(), false)
        );
        assert!(tree.is_dir(&path(&["New", "Deep"])));
    }
}
