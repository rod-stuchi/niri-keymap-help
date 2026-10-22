mod ui;

use std::{env, path::PathBuf, process::ExitCode};

use gtk::{gio, prelude::*};
use niri_keymap_help::{
    config::{self, DescriptionSource},
    sort::SortMode,
};

fn main() -> ExitCode {
    let mut path = config::default_path();
    let mut check = false;
    let mut mode = SortMode::default();
    let mut args = env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--config" | "-c") => match args.next() {
                Some(value) => path = PathBuf::from(value),
                None => {
                    eprintln!("--config requires a file path");
                    return ExitCode::from(2);
                }
            },
            Some("--sort" | "-s") => match args.next().map(|value| value.to_string_lossy().parse())
            {
                Some(Ok(value)) => mode = value,
                Some(Err(error)) => {
                    eprintln!("--sort: {error}");
                    return ExitCode::from(2);
                }
                None => {
                    eprintln!("--sort requires a mode");
                    return ExitCode::from(2);
                }
            },
            Some("--check") => check = true,
            Some("--help" | "-h") => {
                println!(
                    "Niri Keymap Help\n\nUsage: niri-keymap-help [--config PATH] [--sort MODE] [--check]\n\n  -c, --config PATH  Config to read (default: {})\n  -s, --sort MODE    Initial order: grouped (default), config, key, description\n      --check        Parse the config and print counts without opening a window\n  -h, --help         Show this help\n  -V, --version      Show the version\n\nIn the window: type to search; Up/Down and Page Up/Down scroll; Ctrl+S cycles the sort order.\nEscape clears the query, or closes the window when the query is empty.",
                    path.display()
                );
                return ExitCode::SUCCESS;
            }
            Some("--version" | "-V") => {
                println!("niri-keymap-help {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            _ => {
                eprintln!(
                    "Unknown argument: {}. Use --help for usage.",
                    arg.to_string_lossy()
                );
                return ExitCode::from(2);
            }
        }
    }
    if check {
        return match config::load(&path).result {
            Ok(snapshot) => {
                let titled = snapshot
                    .bindings
                    .iter()
                    .filter(|binding| binding.description_source == Some(DescriptionSource::Title))
                    .count();
                let comments = snapshot
                    .bindings
                    .iter()
                    .filter(|binding| {
                        binding.description_source == Some(DescriptionSource::Comment)
                    })
                    .count();
                println!(
                    "{} bindings: {titled} titled, {comments} described by comments, {} missing descriptions",
                    snapshot.bindings.len(),
                    snapshot.bindings.len() - titled - comments
                );
                for warning in snapshot.warnings {
                    eprintln!("Warning: {warning}");
                }
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("{error}");
                ExitCode::FAILURE
            }
        };
    }
    // Single instance: launching again (e.g. pressing the Niri bind twice)
    // raises the existing window instead of opening another one.
    let app = gtk::Application::builder()
        .application_id("io.github.niri_keymap_help")
        .flags(gio::ApplicationFlags::default())
        .build();
    app.connect_activate(move |app| match app.windows().first() {
        Some(window) => window.present(),
        None => ui::build(app, &path, mode),
    });
    let code = app.run_with_args::<&str>(&[]);
    code.into()
}
