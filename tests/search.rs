use niri_keymap_help::{
    config::{Binding, DescriptionSource},
    search,
};

fn binding() -> Binding {
    Binding {
        keys: "Mod+Shift+T".into(),
        description: Some("Open a Terminal — São Paulo".into()),
        description_source: Some(DescriptionSource::Title),
        action: "spawn \"kitty\"".into(),
        file: "config.kdl".into(),
        line: 1,
    }
}

#[test]
fn all_terms_match_across_keys_description_and_action_ignoring_case() {
    for query in [
        "",
        "   ",
        "MOD+SHIFT",
        "terminal",
        "mod terminal",
        "SÃO",
        "t\topen",
        "spawn",
        "kitty",
        "SPAWN KITTY",
        "mod terminal kitty",
    ] {
        assert!(search::matches(&binding(), query), "{query}");
    }
    for query in ["terminal missing", "ctrl", "spawn missing", "kitty ctrl"] {
        assert!(!search::matches(&binding(), query), "{query}");
    }
}

#[test]
fn undocumented_binding_placeholder_does_not_become_searchable_description() {
    let mut binding = binding();
    binding.description = None;
    binding.description_source = None;
    assert!(search::matches(&binding, "shift"));
    assert!(search::matches(&binding, "spawn kitty"));
    assert!(search::matches(&binding, "SHIFT KITTY"));
    assert!(!search::matches(&binding, "description"));
}

#[test]
fn actions_paths_and_arguments_are_searchable() {
    let mut binding = binding();
    binding.action = r#"spawn-sh "$HOME/.scripts/niri-volume.sh up --device speaker""#.into();
    for query in [
        "spawn",
        "spawn-sh",
        "NIRI-VOLUME.SH",
        ".scripts/",
        "--device speaker",
        "mod up",
    ] {
        assert!(search::matches(&binding, query), "{query}");
    }
    assert!(!search::matches(&binding, "spawn-sh down"));

    binding.action = "focus-workspace 3".into();
    assert!(search::matches(&binding, "FOCUS-WORKSPACE 3"));
    assert!(!search::matches(&binding, "spawn"));
}
