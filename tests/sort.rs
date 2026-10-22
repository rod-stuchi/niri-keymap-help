use niri_keymap_help::{
    config::{Binding, DescriptionSource},
    sort::{self, SortMode},
};

fn binding(keys: &str, description: Option<&str>) -> Binding {
    Binding {
        keys: keys.into(),
        description: description.map(Into::into),
        description_source: description.map(|_| DescriptionSource::Title),
        action: "spawn \"kitty\"".into(),
        file: "config.kdl".into(),
        line: 1,
    }
}

fn keys(bindings: &[Binding], mode: SortMode) -> Vec<&str> {
    sort::order(bindings, mode, "")
        .into_iter()
        .map(|index| bindings[index].keys.as_str())
        .collect()
}

#[test]
fn split_group_accepts_short_labels_only() {
    assert_eq!(
        sort::split_group("Focus: column left"),
        Some(("Focus", "column left"))
    );
    assert_eq!(
        sort::split_group("Move to: monitor right"),
        Some(("Move to", "monitor right"))
    );
    assert_eq!(sort::split_group("Close window."), None);
    assert_eq!(sort::split_group("Open the very best: terminal"), None);
    assert_eq!(sort::split_group("Focus: "), None);
    assert_eq!(sort::split_group("See https://example.com"), None);
    assert_eq!(sort::split_group("5 s: back"), None);
}

#[test]
fn grouped_order_follows_first_appearance_and_keeps_config_order() {
    let bindings = [
        binding("A", Some("Focus: left")),
        binding("B", None),
        binding("C", Some("Move: left")),
        binding("D", Some("Plain sentence.")),
        binding("E", Some("Focus: right")),
        binding("F", Some("Move: right")),
    ];
    assert_eq!(
        keys(&bindings, SortMode::Grouped),
        ["A", "E", "C", "F", "D", "B"]
    );
    assert_eq!(sort::group_of(&bindings[1]), sort::UNDOCUMENTED);
    assert_eq!(sort::group_of(&bindings[3]), sort::OTHER);
}

#[test]
fn key_and_description_sorts_ignore_case_and_are_stable() {
    let bindings = [
        binding("mod+b", Some("beta")),
        binding("Mod+A", None),
        binding("Mod+B", Some("Alpha")),
        binding("mod+a", Some("alpha")),
    ];
    assert_eq!(
        keys(&bindings, SortMode::Key),
        ["Mod+A", "mod+a", "mod+b", "Mod+B"]
    );
    assert_eq!(
        keys(&bindings, SortMode::Description),
        ["Mod+B", "mod+a", "mod+b", "Mod+A"]
    );
    assert_eq!(
        keys(&bindings, SortMode::Config),
        ["mod+b", "Mod+A", "Mod+B", "mod+a"]
    );
}

#[test]
fn sort_mode_parses_names() {
    for mode in SortMode::ALL {
        assert_eq!(mode.name().parse::<SortMode>(), Ok(mode));
    }
    assert_eq!("KEY".parse::<SortMode>(), Ok(SortMode::Key));
    assert!("bogus".parse::<SortMode>().is_err());
    assert_eq!(SortMode::default(), SortMode::Grouped);
}

#[test]
fn grouped_order_moves_searched_groups_first() {
    let bindings = [
        binding("A", Some("Window: toggle floating")),
        binding("B", Some("Keyboard: next layout")),
        binding("C", Some("Toggle: blur")),
        binding("D", Some("Toggle: shadows")),
        binding("E", Some("Window: close")),
    ];
    let order = |query| -> Vec<&str> {
        sort::order(&bindings, SortMode::Grouped, query)
            .into_iter()
            .map(|index| bindings[index].keys.as_str())
            .collect()
    };
    assert_eq!(order("TOGGLE"), ["C", "D", "A", "E", "B"]);
    assert_eq!(order("next tog"), ["C", "D", "A", "E", "B"]);
    assert_eq!(order("zzz"), ["A", "E", "B", "C", "D"]);
    assert_eq!(
        sort::order(&bindings, SortMode::Config, "toggle"),
        [0, 1, 2, 3, 4]
    );
}
