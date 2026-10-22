use crate::config::Binding;

/// Each whitespace-separated term may match the keys, description, or action.
/// All terms must match. An empty query includes every binding.
pub fn matches(binding: &Binding, query: &str) -> bool {
    let keys = binding.keys.to_lowercase();
    let description = binding.description.as_deref().unwrap_or("").to_lowercase();
    let action = binding.action.to_lowercase();
    query
        .to_lowercase()
        .split_whitespace()
        .all(|term| keys.contains(term) || description.contains(term) || action.contains(term))
}
