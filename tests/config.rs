use std::{fs, path::Path};

use niri_keymap_help::config::{self, DescriptionSource, Snapshot};
use tempfile::TempDir;

fn parse(source: &str) -> Snapshot {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("config.kdl");
    fs::write(&path, source).unwrap();
    config::load(&path).result.unwrap()
}

fn write(dir: &Path, name: &str, source: &str) {
    fs::write(dir.join(name), source).unwrap();
}

#[test]
fn descriptions_prefer_titles_and_only_use_adjacent_comments() {
    let snapshot = parse(
        r#"
binds {
    // This should lose to the title.
    Mod+T hotkey-overlay-title="Open a terminal" { spawn "kitty"; }
    // Focus the column
    // to the left.
    Mod+H { focus-column-left; }
    Mod+L { focus-column-right; }
    // This comment is separated.

    Mod+Q { close-window; }
    // ===== Audio =====
    XF86AudioMute { spawn "mute"; }
    // Mod+J { focus-window-down; }
    Mod+K { focus-window-up; }
    /* Change the
     * wallpaper. */
    Mod+W { spawn "wallpaper"; }
}
"#,
    );
    let bindings = &snapshot.bindings;
    assert_eq!(bindings.len(), 7);
    assert_eq!(bindings[0].description.as_deref(), Some("Open a terminal"));
    assert_eq!(
        bindings[0].description_source,
        Some(DescriptionSource::Title)
    );
    assert_eq!(bindings[0].line, 4);
    assert_eq!(
        bindings[1].description.as_deref(),
        Some("Focus the column to the left.")
    );
    assert_eq!(
        bindings[1].description_source,
        Some(DescriptionSource::Comment)
    );
    for binding in &bindings[2..6] {
        assert_eq!(binding.description, None, "{}", binding.keys);
    }
    assert_eq!(
        bindings[6].description.as_deref(),
        Some("Change the wallpaper.")
    );
}

#[test]
fn disabled_nodes_properties_and_actions_are_not_used() {
    let snapshot = parse(
        r#"
/-binds { Mod+X { quit; }; }
binds {
    /-Mod+T hotkey-overlay-title="Disabled" { spawn "kitty"; }
    // Disabled example should not become a description.
    /-Mod+L { focus-column-right; }
    Mod+H /-hotkey-overlay-title="Disabled title" {
        /-spawn "ignored"
        focus-column-left
    }
    Mod+W { spawn "with spaces" "{ braces }"; }
}
"#,
    );
    assert_eq!(snapshot.bindings.len(), 2);
    assert_eq!(snapshot.bindings[0].description, None);
    assert_eq!(snapshot.bindings[0].action, "focus-column-left");
    assert_eq!(
        snapshot.bindings[1].action,
        r#"spawn "with spaces" "{ braces }""#
    );
}

#[test]
fn empty_and_null_titles_fall_back_and_markup_is_searchable_text() {
    let snapshot = parse(
        r##"
binds {
    // Close this window.
    Mod+Q hotkey-overlay-title=null { close-window; }
    // Open the terminal.
    Mod+T hotkey-overlay-title="  " { spawn "kitty"; }
    Mod+F hotkey-overlay-title="<b>Fullscreen</b> &amp; focus" { fullscreen-window; }
    Mod+A hotkey-overlay-title="Use <invalid> literally" { spawn "a"; }
    Mod+N hotkey-overlay-title=null { spawn "n"; }
    Mod+R hotkey-overlay-title=r#"Say "hello""# { spawn "r"; }
}
"##,
    );
    assert_eq!(
        snapshot.bindings[0].description.as_deref(),
        Some("Close this window.")
    );
    assert_eq!(
        snapshot.bindings[1].description.as_deref(),
        Some("Open the terminal.")
    );
    assert_eq!(
        snapshot.bindings[2].description.as_deref(),
        Some("Fullscreen & focus")
    );
    assert_eq!(
        snapshot.bindings[3].description.as_deref(),
        Some("Use <invalid> literally")
    );
    assert_eq!(snapshot.bindings[4].description, None);
    assert_eq!(
        snapshot.bindings[5].description.as_deref(),
        Some("Say \"hello\"")
    );
}

#[test]
fn include_order_relative_paths_and_modifier_aliases_match_niri() {
    let dir = TempDir::new().unwrap();
    fs::create_dir(dir.path().join("parts")).unwrap();
    write(
        dir.path(),
        "config.kdl",
        r#"
binds { Ctrl+Shift+T { spawn "first"; }; }
include "parts/keys.kdl"
binds { Shift+Control+t hotkey-overlay-title="Last wins" { spawn "last"; }; }
"#,
    );
    write(
        dir.path(),
        "parts/keys.kdl",
        r#"
binds {
    Shift+Ctrl+t { spawn "middle"; }
    Mod+L { focus-column-right; }
}
include "more.kdl"
"#,
    );
    write(
        dir.path(),
        "parts/more.kdl",
        "binds { Mod+H { focus-column-left; }; }",
    );
    let report = config::load(&dir.path().join("config.kdl"));
    assert_eq!(report.files.len(), 3);
    let snapshot = report.result.unwrap();
    assert_eq!(snapshot.bindings.len(), 3);
    assert_eq!(snapshot.bindings[0].action, "spawn \"last\"");
    assert_eq!(snapshot.bindings[2].file, dir.path().join("parts/more.kdl"));
}

#[test]
fn optional_missing_files_are_tracked_but_invalid_optional_files_fail() {
    let dir = TempDir::new().unwrap();
    write(
        dir.path(),
        "config.kdl",
        "include optional=true \"optional.kdl\"\n",
    );
    let report = config::load(&dir.path().join("config.kdl"));
    assert!(report.files.contains(&dir.path().join("optional.kdl")));
    assert_eq!(report.result.unwrap().warnings.len(), 1);
    write(dir.path(), "optional.kdl", "binds {\n");
    let error = config::load(&dir.path().join("config.kdl"))
        .result
        .unwrap_err();
    assert!(error.contains("optional.kdl:"), "{error}");
}

#[test]
fn required_missing_includes_and_cycles_produce_actionable_errors() {
    let dir = TempDir::new().unwrap();
    write(dir.path(), "config.kdl", "include \"missing.kdl\"\n");
    let report = config::load(&dir.path().join("config.kdl"));
    assert!(report.files.contains(&dir.path().join("missing.kdl")));
    assert!(report.result.unwrap_err().contains("missing.kdl"));
    write(dir.path(), "missing.kdl", "include \"config.kdl\"\n");
    assert!(
        config::load(&dir.path().join("config.kdl"))
            .result
            .unwrap_err()
            .contains("cycle")
    );
}

#[test]
fn duplicate_bindings_in_one_block_and_invalid_titles_fail() {
    let dir = TempDir::new().unwrap();
    for (source, expected) in [
        (
            "binds { Mod+T { spawn \"a\"; }; Mod+t { spawn \"b\"; }; }",
            "duplicate key",
        ),
        (
            "binds { Mod+T hotkey-overlay-title=42 { spawn \"a\"; }; }",
            "string or null",
        ),
        ("binds { Mod+T { }; }", "exactly one action"),
    ] {
        write(dir.path(), "config.kdl", source);
        let error = config::load(&dir.path().join("config.kdl"))
            .result
            .unwrap_err();
        assert!(error.contains(expected), "{error}");
        assert!(error.contains("config.kdl:1:"), "{error}");
    }
}

#[test]
fn separate_bindings_for_the_same_action_are_preserved() {
    let snapshot = parse("binds { Mod+T { spawn \"kitty\"; }; Mod+Return { spawn \"kitty\"; }; }");
    assert_eq!(snapshot.bindings.len(), 2);
}

#[test]
fn inline_comments_do_not_leak_to_the_next_binding() {
    let snapshot = parse(
        "binds {\n Mod+T { spawn \"kitty\"; } // Launch terminal\n Mod+Q { close-window; }\n}\n",
    );
    assert_eq!(snapshot.bindings[1].description, None);
}
