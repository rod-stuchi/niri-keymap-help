use std::{
    collections::{BTreeSet, HashMap, HashSet},
    env, fs,
    path::{Path, PathBuf},
};

use kdl::{KdlDocument, KdlNode, KdlValue};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DescriptionSource {
    Title,
    Comment,
}

#[derive(Clone, Debug)]
pub struct Binding {
    pub keys: String,
    pub description: Option<String>,
    pub description_source: Option<DescriptionSource>,
    pub action: String,
    pub file: PathBuf,
    pub line: usize,
}

#[derive(Debug)]
pub struct Snapshot {
    pub bindings: Vec<Binding>,
    pub warnings: Vec<String>,
}

/// Dependencies are returned even on failure so that fixing a broken include
/// (or creating a missing one) triggers another load.
#[derive(Debug)]
pub struct LoadReport {
    pub result: Result<Snapshot, String>,
    pub files: BTreeSet<PathBuf>,
}

pub fn default_path() -> PathBuf {
    env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| {
            env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".config")
        })
        .join("niri/config.kdl")
}

pub fn load(path: &Path) -> LoadReport {
    let mut loader = Loader::default();
    let result = loader.read(path, false).map(|()| Snapshot {
        bindings: loader.bindings,
        warnings: loader.warnings,
    });
    LoadReport {
        result,
        files: loader.files,
    }
}

#[derive(Default)]
struct Loader {
    bindings: Vec<Binding>,
    positions: HashMap<String, usize>,
    files: BTreeSet<PathBuf>,
    stack: Vec<PathBuf>,
    warnings: Vec<String>,
}

impl Loader {
    fn read(&mut self, path: &Path, optional: bool) -> Result<(), String> {
        self.files.insert(path.to_path_buf());
        let source = match fs::read_to_string(path) {
            Ok(source) => source,
            Err(error) if optional && error.kind() == std::io::ErrorKind::NotFound => {
                self.warnings
                    .push(format!("Optional include not found: {}", path.display()));
                return Ok(());
            }
            Err(error) => return Err(format!("{}: {error}", path.display())),
        };
        let canonical =
            fs::canonicalize(path).map_err(|error| format!("{}: {error}", path.display()))?;
        if self.stack.contains(&canonical) {
            return Err(format!("{}: include cycle detected", path.display()));
        }
        if self.stack.len() >= 64 {
            return Err(format!(
                "{}: include nesting exceeds 64 files",
                path.display()
            ));
        }
        let document: KdlDocument = source.parse().map_err(|error: kdl::KdlError| {
            let (line, column) = location(&source, error.span.offset());
            format!("{}:{line}:{column}: {error}", path.display())
        })?;
        self.stack.push(canonical);
        let result = self.read_document(&document, &source, path);
        self.stack.pop();
        result
    }

    fn read_document(
        &mut self,
        document: &KdlDocument,
        source: &str,
        path: &Path,
    ) -> Result<(), String> {
        for node in document.nodes() {
            match node.name().value() {
                "include" => {
                    let filename = node
                        .get(0)
                        .and_then(|entry| entry.value().as_string())
                        .ok_or_else(|| {
                            node_error(node, source, path, "include needs a file path")
                        })?;
                    let optional = match node.get("optional").map(|entry| entry.value()) {
                        None => false,
                        Some(KdlValue::Bool(value)) => *value,
                        _ => {
                            return Err(node_error(
                                node,
                                source,
                                path,
                                "optional must be a boolean",
                            ));
                        }
                    };
                    let include = if let Some(rest) = filename.strip_prefix("~/") {
                        PathBuf::from(env::var_os("HOME").ok_or("HOME is not set")?).join(rest)
                    } else {
                        path.parent().unwrap_or(Path::new(".")).join(filename)
                    };
                    self.read(&include, optional)?;
                }
                "binds" => {
                    if let Some(children) = node.children() {
                        let mut seen = HashSet::new();
                        for binding in children.nodes() {
                            let identity = key_identity(binding.name().value());
                            if !seen.insert(identity.clone()) {
                                return Err(node_error(
                                    binding,
                                    source,
                                    path,
                                    "duplicate key in this binds block",
                                ));
                            }
                            let value = parse_binding(binding, source, path)?;
                            if let Some(&index) = self.positions.get(&identity) {
                                self.bindings[index] = value;
                            } else {
                                self.positions.insert(identity, self.bindings.len());
                                self.bindings.push(value);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}

fn parse_binding(node: &KdlNode, source: &str, path: &Path) -> Result<Binding, String> {
    let title = match node.get("hotkey-overlay-title").map(|entry| entry.value()) {
        None | Some(KdlValue::Null) => None,
        Some(value) => Some(value.as_string().ok_or_else(|| {
            node_error(
                node,
                source,
                path,
                "hotkey-overlay-title must be a string or null",
            )
        })?),
    };
    // Niri titles support Pango markup. Search the visible text, and use our
    // own styling consistently. Invalid markup is displayed literally.
    let title = title
        .map(|title| {
            gtk::pango::parse_markup(title, '\0')
                .map(|(_, text, _)| text.to_string())
                .unwrap_or_else(|_| title.to_owned())
        })
        .filter(|title| !title.trim().is_empty());
    let (description, description_source) = if let Some(title) = title {
        (
            Some(title.trim().to_owned()),
            Some(DescriptionSource::Title),
        )
    } else {
        let comment = preceding_comment(source, node.span().offset());
        let origin = comment.as_ref().map(|_| DescriptionSource::Comment);
        (comment, origin)
    };
    let actions = node
        .children()
        .map(|children| children.nodes())
        .unwrap_or(&[]);
    if actions.len() != 1 {
        return Err(node_error(
            node,
            source,
            path,
            "a binding must contain exactly one action",
        ));
    }
    Ok(Binding {
        keys: node.name().value().to_owned(),
        description,
        description_source,
        action: format_action(&actions[0]),
        file: path.to_path_buf(),
        line: location(source, node.span().offset()).0,
    })
}

fn format_action(node: &KdlNode) -> String {
    let mut parts = vec![node.name().value().to_owned()];
    for entry in node.entries() {
        let value = entry.value().to_string();
        parts.push(match entry.name() {
            Some(name) => format!("{}={value}", name.value()),
            None => value,
        });
    }
    if let Some(children) = node.children() {
        parts.push(format!(
            "{{ {} }}",
            children
                .nodes()
                .iter()
                .map(format_action)
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }
    parts.join(" ")
}

fn location(source: &str, offset: usize) -> (usize, usize) {
    let prefix = &source[..offset.min(source.len())];
    let line = prefix.bytes().filter(|&byte| byte == b'\n').count() + 1;
    let column = prefix.rsplit('\n').next().unwrap_or("").chars().count() + 1;
    (line, column)
}

fn node_error(node: &KdlNode, source: &str, path: &Path, message: &str) -> String {
    let (line, column) = location(source, node.span().offset());
    format!("{}:{line}:{column}: {message}", path.display())
}

/// Associate only an adjacent, standalone // or /* */ block. A blank line,
/// code, or decorative heading is a boundary; comments never leak to the
/// next binding. Source spans keep disabled KDL nodes out of this process.
fn preceding_comment(source: &str, offset: usize) -> Option<String> {
    let prefix = &source[..offset];
    let line_start = prefix.rfind('\n').map_or(0, |index| index + 1);
    if !prefix[line_start..].trim().is_empty() {
        return None;
    }
    let mut lines = prefix[..line_start].lines().rev().peekable();
    let mut comments = Vec::new();
    while let Some(line) = lines.next() {
        let line = line.trim();
        if let Some(text) = line.strip_prefix("//") {
            let text = text.trim();
            if is_comment_boundary(text) {
                break;
            }
            comments.push(text.to_owned());
        } else if let Some(end) = line.strip_suffix("*/") {
            let mut block = vec![end.to_owned()];
            while !block.last().unwrap().trim_start().starts_with("/*") {
                block.push(lines.next()?.trim().to_owned());
            }
            block.reverse();
            let first = block.first_mut().unwrap();
            *first = first.trim_start().strip_prefix("/*")?.to_owned();
            // Reject inline code and nested comments conservatively.
            let text = block
                .iter()
                .map(|line| line.trim().trim_start_matches('*').trim())
                .collect::<Vec<_>>()
                .join(" ");
            if text.contains("/*") || text.contains("*/") || is_comment_boundary(&text) {
                break;
            }
            comments.push(text);
        } else {
            break;
        }
    }
    comments.reverse();
    let comment = comments.join(" ");
    (!comment.trim().is_empty()).then_some(comment)
}

fn is_comment_boundary(text: &str) -> bool {
    text.is_empty()
        || text.starts_with("/-")
        || text.contains('{')
        || text.contains('}')
        || text.contains("===")
        || text.contains("---")
        || !text.chars().any(char::is_alphanumeric)
}

/// Niri compares modifiers as a set, recognizes aliases, and resolves keys
/// case-insensitively. Keep Mod distinct from Super (nested Niri uses Alt).
fn key_identity(keys: &str) -> String {
    let mut parts = keys.rsplitn(2, '+');
    let trigger = parts.next().unwrap_or("");
    let mut modifiers = parts
        .next()
        .unwrap_or("")
        .split('+')
        .filter(|part| !part.is_empty())
        .map(|part| match part.trim().to_ascii_lowercase().as_str() {
            "control" => "ctrl".to_owned(),
            "win" => "super".to_owned(),
            "mod5" => "iso_level3_shift".to_owned(),
            "mod3" => "iso_level5_shift".to_owned(),
            other => other.to_owned(),
        })
        .collect::<Vec<_>>();
    modifiers.sort();
    modifiers.dedup();
    let key = gtk::gdk::Key::from_name(trigger)
        .and_then(|key| key.to_lower().name())
        .map(|name| name.to_string())
        .unwrap_or_else(|| trigger.to_owned());
    // These two XKB symbols are distinct and Niri special-cases their case.
    let key = if trigger == "XF86ScreenSaver" || trigger == "XF86Screensaver" {
        trigger.to_owned()
    } else {
        key.to_ascii_lowercase()
    };
    format!("{}+{key}", modifiers.join("+"))
}
