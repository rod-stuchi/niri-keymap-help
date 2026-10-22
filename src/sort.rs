use std::{collections::HashMap, str::FromStr};

use crate::config::Binding;

pub const OTHER: &str = "Other";
pub const UNDOCUMENTED: &str = "No description";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SortMode {
    #[default]
    Grouped,
    Config,
    Key,
    Description,
}

impl SortMode {
    pub const ALL: [SortMode; 4] = [
        SortMode::Grouped,
        SortMode::Config,
        SortMode::Key,
        SortMode::Description,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SortMode::Grouped => "Grouped",
            SortMode::Config => "Config order",
            SortMode::Key => "Key",
            SortMode::Description => "Description",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            SortMode::Grouped => "grouped",
            SortMode::Config => "config",
            SortMode::Key => "key",
            SortMode::Description => "description",
        }
    }
}

impl FromStr for SortMode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, String> {
        SortMode::ALL
            .into_iter()
            .find(|mode| mode.name().eq_ignore_ascii_case(value))
            .ok_or_else(|| {
                format!(
                    "unknown sort mode {value:?}; expected grouped, config, key, or description"
                )
            })
    }
}

/// Split a `Group: action` description. Only a short leading label counts as
/// a group, so ordinary sentences that happen to contain a colon stay whole.
pub fn split_group(description: &str) -> Option<(&str, &str)> {
    let (group, rest) = description.split_once(": ")?;
    let (group, rest) = (group.trim(), rest.trim());
    let words = group.split_whitespace().count();
    let valid = (1..=2).contains(&words)
        && group.chars().count() <= 24
        && group.starts_with(char::is_alphabetic)
        && !rest.is_empty();
    valid.then_some((group, rest))
}

pub fn group_of(binding: &Binding) -> &str {
    match binding.description.as_deref() {
        None => UNDOCUMENTED,
        Some(description) => split_group(description).map_or(OTHER, |(group, _)| group),
    }
}

/// Display order as indices into `bindings`. Every mode falls back to config
/// order, so equal entries keep the layout of the config file.
pub fn order(bindings: &[Binding], mode: SortMode) -> Vec<usize> {
    let mut indices: Vec<usize> = (0..bindings.len()).collect();
    match mode {
        SortMode::Config => {}
        SortMode::Grouped => {
            // Groups appear in the order the config first mentions them;
            // ungrouped and undocumented bindings always come last.
            let mut rank = HashMap::new();
            for binding in bindings {
                let next = rank.len();
                rank.entry(group_of(binding)).or_insert(next);
            }
            let key = |group: &str| match group {
                UNDOCUMENTED => (2, 0),
                OTHER => (1, 0),
                group => (0, rank[group]),
            };
            indices.sort_by_key(|&index| key(group_of(&bindings[index])));
        }
        SortMode::Key => indices.sort_by_cached_key(|&index| bindings[index].keys.to_lowercase()),
        SortMode::Description => indices.sort_by_cached_key(|&index| {
            let description = bindings[index].description.as_deref();
            (
                description.is_none(),
                description.unwrap_or("").to_lowercase(),
            )
        }),
    }
    indices
}
