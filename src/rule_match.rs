use std::path::PathBuf;

use crate::{
    favorites::{Rule, RuleKind},
    inbox::{EntryKind, InboxEntry},
};

/// The read-only result of applying the ordered Rules to one Inbox Entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RuleMatch {
    Unmatched,
    Suggested(SuggestedDestination),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SuggestedDestination {
    rule_index: usize,
    favorite_name: String,
    path: PathBuf,
}

impl SuggestedDestination {
    #[allow(dead_code)] // Presentation is introduced by the following milestone slice.
    pub fn rule_index(&self) -> usize {
        self.rule_index
    }
    #[allow(dead_code)]
    pub fn favorite_name(&self) -> &str {
        &self.favorite_name
    }
    #[allow(dead_code)]
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

pub fn evaluate(
    entry: &InboxEntry,
    rules: &[Rule],
    favorite_path: impl Fn(&str) -> Option<PathBuf>,
) -> RuleMatch {
    let Some(name) = entry.path().file_name().and_then(|name| name.to_str()) else {
        return RuleMatch::Unmatched;
    };
    let Some((rule_index, rule)) = rules
        .iter()
        .enumerate()
        .find(|(_, rule)| kind_matches(entry, rule.kind()) && glob_matches(rule.pattern(), name))
    else {
        return RuleMatch::Unmatched;
    };
    favorite_path(rule.favorite_name())
        .map(|path| SuggestedDestination {
            rule_index,
            favorite_name: rule.favorite_name().to_owned(),
            path,
        })
        .map(RuleMatch::Suggested)
        .unwrap_or(RuleMatch::Unmatched)
}

fn kind_matches(entry: &InboxEntry, kind: RuleKind) -> bool {
    match kind {
        RuleKind::Any => true,
        RuleKind::Symlink => entry.is_symlink(),
        RuleKind::File => !entry.is_symlink() && entry.kind() == EntryKind::File,
        RuleKind::Directory => !entry.is_symlink() && entry.kind() == EntryKind::Directory,
    }
}

/// Matches a case-sensitive basename glob with `*`, `?`, bracket character
/// classes, ranges, and `!`/`^` class negation. An unclosed `[` is literal.
pub fn glob_matches(pattern: &str, name: &str) -> bool {
    let pattern = pattern.chars().collect::<Vec<_>>();
    let name = name.chars().collect::<Vec<_>>();
    matches_from(&pattern, &name, 0, 0)
}

fn matches_from(pattern: &[char], name: &[char], pattern_index: usize, name_index: usize) -> bool {
    if pattern_index == pattern.len() {
        return name_index == name.len();
    }
    match pattern[pattern_index] {
        '*' => (name_index..=name.len())
            .any(|index| matches_from(pattern, name, pattern_index + 1, index)),
        '?' => {
            name_index < name.len()
                && matches_from(pattern, name, pattern_index + 1, name_index + 1)
        }
        '[' => match class_end(pattern, pattern_index) {
            Some(end)
                if name_index < name.len()
                    && class_matches(&pattern[pattern_index + 1..end], name[name_index]) =>
            {
                matches_from(pattern, name, end + 1, name_index + 1)
            }
            Some(_) => false,
            None => {
                name.get(name_index) == Some(&'[')
                    && matches_from(pattern, name, pattern_index + 1, name_index + 1)
            }
        },
        literal => {
            name.get(name_index) == Some(&literal)
                && matches_from(pattern, name, pattern_index + 1, name_index + 1)
        }
    }
}

fn class_end(pattern: &[char], start: usize) -> Option<usize> {
    (start + 1..pattern.len()).find(|index| pattern[*index] == ']')
}

fn class_matches(class: &[char], character: char) -> bool {
    let (negated, class) = match class.first() {
        Some('!' | '^') => (true, &class[1..]),
        _ => (false, class),
    };
    let mut matched = false;
    let mut index = 0;
    while index < class.len() {
        if index + 2 < class.len() && class[index + 1] == '-' {
            matched |= class[index] <= character && character <= class[index + 2];
            index += 3;
        } else {
            matched |= class[index] == character;
            index += 1;
        }
    }
    matched != negated
}

#[cfg(test)]
mod tests {
    use super::glob_matches;

    #[test]
    fn glob_supports_wildcards_character_classes_and_case_sensitivity() {
        assert!(glob_matches("*.rs", "main.rs"));
        assert!(glob_matches("file?.[ch]", "file1.c"));
        assert!(glob_matches("[!a-c]*", "Zed"));
        assert!(glob_matches("[^a-c]*", "Zed"));
        assert!(!glob_matches("*.RS", "main.rs"));
        assert!(!glob_matches("file?.[ch]", "file12.c"));
        assert!(glob_matches("a[", "a["));
    }
}
