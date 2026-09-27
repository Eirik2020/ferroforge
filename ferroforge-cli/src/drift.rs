//! `ferroforge drift`: catch copies of the same code going out of step.
//!
//! Some code has to be duplicated between firmwares because it cannot become a
//! reusable task - an `init` fragment, a resource layout, a task declaration.
//! Marking each copy with the same name lets this compare them:
//!
//! ```text
//! // ferroforge:begin control_loop
//! ...
//! // ferroforge:end control_loop
//! ```
//!
//! The comparison is one to one after normalizing what does not change the
//! code's meaning to a reader: indentation, blank lines and whole-line `//`
//! comments. Marker lines are kept, so where a nested region sits is part of
//! its outer region. Lines are compared rather than tokens so a region may be
//! any fragment, including one that starts halfway through a block, and so a
//! difference can be reported at a line someone can open.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

use serde::Deserialize;

use crate::{all::Style, project::Project};

const BEGIN: &str = "ferroforge:begin";
const END: &str = "ferroforge:end";

/// The project-wide file drift reads its rules from, beside `firmware/`.
pub const SETTINGS_FILE: &str = "ferroforge.toml";

/// The project file. Unknown keys are refused, as in a firmware's manifest: a
/// misspelt rule that is silently ignored reports drift it was meant to forgive.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectFile {
    #[serde(default)]
    drift: Rules,
}

/// Differences expected between firmwares, forgiven in every region. Without
/// any, copies are compared one to one.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct Rules {
    /// `binds = TIM2` and `binds = TIM3` are the same line.
    #[serde(default)]
    ignore_interrupt_bindings: bool,
    /// Any word matching one of these is the same word: `TIM*`, `p??`.
    #[serde(default)]
    ignore_words: Vec<String>,
}

impl Rules {
    fn read(root: &Path) -> Result<Self, String> {
        let path = root.join(SETTINGS_FILE);
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => return Err(format!("{SETTINGS_FILE}: cannot be read: {error}")),
        };
        let file: ProjectFile =
            toml::from_str(&text).map_err(|error| format!("{SETTINGS_FILE}: {error}"))?;
        for pattern in &file.drift.ignore_words {
            if pattern.chars().all(|c| c == '*' || c == '?') {
                return Err(format!(
                    "{SETTINGS_FILE}: `ignore-words` pattern `{pattern}` matches every \
                     word, which would compare nothing but punctuation; name at least \
                     one character"
                ));
            }
            if let Some(c) = pattern
                .chars()
                .find(|c| !(c.is_alphanumeric() || matches!(c, '_' | '*' | '?')))
            {
                return Err(format!(
                    "{SETTINGS_FILE}: `ignore-words` pattern `{pattern}` holds `{c}`; a \
                     pattern is one word, with `*` for any run of characters and `?` \
                     for one"
                ));
            }
        }
        Ok(file.drift)
    }

    fn is_empty(&self) -> bool {
        !self.ignore_interrupt_bindings && self.ignore_words.is_empty()
    }

    /// What is forgiven, for the line that says so above the results.
    fn describe(&self) -> String {
        let mut parts = Vec::new();
        if self.ignore_interrupt_bindings {
            parts.push("interrupt bindings".to_owned());
        }
        if !self.ignore_words.is_empty() {
            parts.push(format!("words matching {}", self.ignore_words.join(", ")));
        }
        parts.join(" and ")
    }

    /// The line as the comparison sees it: every forgiven difference replaced
    /// by the same `_`, so copies that differ only there compare equal.
    fn key(&self, line: &str) -> String {
        let mut words = String::with_capacity(line.len());
        let mut chars = line.char_indices().peekable();
        while let Some((start, c)) = chars.next() {
            if !(c.is_alphanumeric() || c == '_') {
                words.push(c);
                continue;
            }
            let mut end = start + c.len_utf8();
            while let Some(&(index, next)) = chars.peek() {
                if !(next.is_alphanumeric() || next == '_') {
                    break;
                }
                end = index + next.len_utf8();
                chars.next();
            }
            let word = &line[start..end];
            let forgiven = self
                .ignore_words
                .iter()
                .any(|pattern| matches_pattern(pattern, word));
            words.push_str(if forgiven { "_" } else { word });
        }
        match self.ignore_interrupt_bindings {
            true => without_bindings(&words),
            false => words,
        }
    }
}

/// `*` is any run of characters, `?` exactly one; anything else is itself.
fn matches_pattern(pattern: &str, word: &str) -> bool {
    fn from(pattern: &[char], word: &[char]) -> bool {
        match pattern.split_first() {
            None => word.is_empty(),
            Some(('*', rest)) => (0..=word.len()).any(|skip| from(rest, &word[skip..])),
            Some(('?', rest)) => !word.is_empty() && from(rest, &word[1..]),
            Some((c, rest)) => word.first() == Some(c) && from(rest, &word[1..]),
        }
    }
    let pattern = pattern.chars().collect::<Vec<_>>();
    let word = word.chars().collect::<Vec<_>>();
    from(&pattern, &word)
}

/// Every `binds = <path>` with its path replaced by `_`.
fn without_bindings(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(found) = rest.find("binds") {
        let before_ok = rest[..found]
            .chars()
            .next_back()
            .is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
        let after = &rest[found + "binds".len()..];
        let assigned = after.trim_start().strip_prefix('=').filter(|_| before_ok);
        let Some(value) = assigned else {
            out.push_str(&rest[..found + "binds".len()]);
            rest = after;
            continue;
        };
        let value = value.trim_start();
        let length = value
            .char_indices()
            .find(|(_, c)| !(c.is_alphanumeric() || matches!(c, '_' | ':')))
            .map_or(value.len(), |(index, _)| index);
        out.push_str(&rest[..found]);
        out.push_str("binds = _");
        rest = &value[length..];
    }
    out.push_str(rest);
    out
}

/// A line of a region: where it is, what it says, and what is compared.
struct Line {
    number: usize,
    text: String,
    key: String,
}

/// One marked region in one firmware.
struct Region {
    /// Path shown in reports, relative to the project.
    file: String,
    begin: usize,
    depth: usize,
    /// Normalized lines with the line number each came from.
    lines: Vec<Line>,
}

enum Marker<'a> {
    Begin(&'a str),
    End(&'a str),
}

/// `// ferroforge:begin <name>` or `// ferroforge:end <name>`, however indented.
fn marker(line: &str) -> Option<Result<Marker<'_>, &'static str>> {
    let comment = line.trim().strip_prefix("//")?.trim();
    let (kind, rest) = match comment.strip_prefix(BEGIN) {
        Some(rest) => (true, rest),
        None => (false, comment.strip_prefix(END)?),
    };
    // `ferroforge:beginning` is not a marker.
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let name = rest.trim();
    if name.is_empty() {
        return Some(Err("a marker needs a region name"));
    }
    if name.contains(char::is_whitespace) {
        return Some(Err("a region name is one word"));
    }
    Some(Ok(if kind {
        Marker::Begin(name)
    } else {
        Marker::End(name)
    }))
}

/// What the comparison sees of a line, or `None` for one it ignores.
fn normalized(line: &str) -> Option<String> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    // Doc comments are part of the item; ordinary comments are notes.
    let documented = line.starts_with("///") || line.starts_with("//!");
    if line.starts_with("//") && !documented && marker(line).is_none() {
        return None;
    }
    Some(line.to_owned())
}

/// Every region in one firmware, keyed by name, plus the problems that stop
/// its markers being read.
fn scan(root: &Path, firmware: &Path, rules: &Rules) -> (BTreeMap<String, Region>, Vec<String>) {
    let mut regions: BTreeMap<String, Region> = BTreeMap::new();
    let mut errors = Vec::new();

    for path in rust_files(&firmware.join("src")) {
        let shown = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .display()
            .to_string()
            .replace('\\', "/");
        let Ok(text) = fs::read_to_string(&path) else {
            errors.push(format!("{shown}: cannot be read"));
            continue;
        };
        // Names of the regions open at this point, innermost last.
        let mut open: Vec<String> = Vec::new();
        for (index, line) in text.lines().enumerate() {
            let number = index + 1;
            let content = normalized(line);
            match marker(line) {
                Some(Err(reason)) => errors.push(format!("{shown}:{number}: {reason}")),
                Some(Ok(Marker::Begin(name))) => {
                    if let Some(first) = regions.get(name) {
                        errors.push(format!(
                            "{shown}:{number}: `{name}` is already marked at {}:{}; \
                             a name marks one region per firmware",
                            first.file, first.begin
                        ));
                    } else {
                        regions.insert(
                            name.to_owned(),
                            Region {
                                file: shown.clone(),
                                begin: number,
                                depth: open.len(),
                                lines: Vec::new(),
                            },
                        );
                    }
                    // Opened even when refused, so its end still matches and
                    // one mistake is reported once.
                    open.push(name.to_owned());
                    push(&mut regions, &open, number, &content, rules);
                }
                Some(Ok(Marker::End(name))) => match open.last() {
                    Some(innermost) if innermost == name => {
                        push(&mut regions, &open, number, &content, rules);
                        open.pop();
                    }
                    Some(innermost) => {
                        errors.push(format!(
                            "{shown}:{number}: `{END} {name}` closes `{name}`, but \
                             `{innermost}` is still open inside it; close the inner \
                             region first"
                        ));
                        // Treated as closed, so its missing end is not reported
                        // a second time.
                        open.retain(|open| open != name);
                    }
                    None => errors.push(format!(
                        "{shown}:{number}: `{END} {name}` has no `{BEGIN} {name}` before it"
                    )),
                },
                None => push(&mut regions, &open, number, &content, rules),
            }
        }
        for name in open {
            let begin = regions.get(&name).map_or(0, |region| region.begin);
            errors.push(format!(
                "{shown}:{begin}: `{BEGIN} {name}` has no `{END} {name}`"
            ));
        }
    }
    (regions, errors)
}

/// Add a line to every region that is open.
fn push(
    regions: &mut BTreeMap<String, Region>,
    open: &[String],
    number: usize,
    content: &Option<String>,
    rules: &Rules,
) {
    let Some(content) = content else { return };
    let key = rules.key(content);
    for name in open {
        if let Some(region) = regions.get_mut(name) {
            region.lines.push(Line {
                number,
                text: content.clone(),
                key: key.clone(),
            });
        }
    }
}

/// In a stable order, so reports read the same way twice.
fn rust_files(directory: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![directory.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

enum Change<'a> {
    Same,
    Removed(&'a Line),
    Added(&'a Line),
}

/// A line diff by longest common subsequence. Regions are short, so the
/// quadratic table costs nothing worth a dependency.
fn diff<'a>(before: &'a [Line], after: &'a [Line]) -> Vec<Change<'a>> {
    let (n, m) = (before.len(), after.len());
    let mut common = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            common[i][j] = if before[i].key == after[j].key {
                common[i + 1][j + 1] + 1
            } else {
                common[i + 1][j].max(common[i][j + 1])
            };
        }
    }
    let (mut i, mut j, mut changes) = (0, 0, Vec::new());
    while i < n || j < m {
        if i < n && j < m && before[i].key == after[j].key {
            changes.push(Change::Same);
            i += 1;
            j += 1;
        } else if i < n && (j == m || common[i + 1][j] >= common[i][j + 1]) {
            // Removed before added, so a replaced line reads old then new.
            changes.push(Change::Removed(&before[i]));
            i += 1;
        } else {
            changes.push(Change::Added(&after[j]));
            j += 1;
        }
    }
    changes
}

fn same(first: &Region, second: &Region) -> bool {
    first.lines.len() == second.lines.len()
        && first
            .lines
            .iter()
            .zip(&second.lines)
            .all(|(a, b)| a.key == b.key)
}

pub fn run(project: &Project) -> Result<ExitCode, String> {
    let firmwares = project.firmwares().map_err(|error| error.to_string())?;
    let style = Style::detect();
    let rules = Rules::read(&project.root)?;

    let mut scanned = Vec::new();
    let mut errors = Vec::new();
    for firmware in &firmwares {
        let (regions, mut found) = scan(&project.root, &firmware.path, &rules);
        errors.append(&mut found);
        scanned.push((firmware.name.as_str(), regions));
    }
    // A region whose markers cannot be read has no content to compare, and
    // comparing the rest would report drift that is only a missing marker.
    if !errors.is_empty() {
        for error in &errors {
            println!("{} {error}", style.paint("31", "error:"));
        }
        return Ok(ExitCode::FAILURE);
    }

    // Outer before inner: every name in the order its region begins, taking
    // the first firmware that marks it as where it sits.
    let mut names: Vec<(&str, &Region)> = Vec::new();
    for (_, regions) in &scanned {
        for (name, region) in regions {
            if !names.iter().any(|(seen, _)| seen == name) {
                names.push((name, region));
            }
        }
    }
    names.sort_by(|(_, a), (_, b)| (&a.file, a.begin).cmp(&(&b.file, b.begin)));

    if names.is_empty() {
        println!("no marked regions; mark copies with `// {BEGIN} <name>` and `// {END} <name>`");
        return Ok(ExitCode::SUCCESS);
    }

    // Said before the results, so a region reported `same` is known to be the
    // same only up to what the project forgives.
    if !rules.is_empty() {
        println!("ignoring {}, per {SETTINGS_FILE}\n", rules.describe());
    }

    let width = names
        .iter()
        .map(|(name, region)| name.len() + 2 * region.depth)
        .max()
        .unwrap_or(0);
    let mut drifted = Vec::new();
    for (name, placed) in &names {
        let holders = scanned
            .iter()
            .filter_map(|(firmware, regions)| regions.get(*name).map(|region| (*firmware, region)))
            .collect::<Vec<_>>();
        let label = format!("{:width$}", format!("{}{name}", "  ".repeat(placed.depth)));
        let listed = holders
            .iter()
            .map(|(firmware, _)| *firmware)
            .collect::<Vec<_>>()
            .join(", ");

        let [(reference_name, reference), others @ ..] = holders.as_slice() else {
            continue;
        };
        if others.is_empty() {
            println!(
                "{label}   {}    only in {reference_name}",
                style.paint("33", "alone")
            );
            continue;
        }
        let differing = others
            .iter()
            .filter(|(_, region)| !same(reference, region))
            .collect::<Vec<_>>();
        if differing.is_empty() {
            println!("{label}   {}     {listed}", style.paint("32", "same"));
            continue;
        }

        println!("{label}   {}    {listed}", style.paint("31", "DRIFT"));
        drifted.push(*name);
        for (firmware_name, region) in differing {
            println!(
                "    {} ({reference_name}) vs {} ({firmware_name})",
                reference.file, region.file
            );
            for change in diff(&reference.lines, &region.lines) {
                match change {
                    Change::Same => {}
                    Change::Removed(Line { number, text, .. }) => println!(
                        "    {}",
                        style.paint("31", &format!("- {number:>4}  {text}"))
                    ),
                    Change::Added(Line { number, text, .. }) => println!(
                        "    {}",
                        style.paint("32", &format!("+ {number:>4}  {text}"))
                    ),
                }
            }
        }
    }

    match drifted.is_empty() {
        true => Ok(ExitCode::SUCCESS),
        false => {
            println!("\ndrift in {}", drifted.join(", "));
            Ok(ExitCode::FAILURE)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &[&str]) -> Vec<Line> {
        text.iter()
            .enumerate()
            .map(|(index, line)| Line {
                number: index + 1,
                text: (*line).to_owned(),
                key: (*line).to_owned(),
            })
            .collect()
    }

    #[test]
    fn markers_are_recognized_however_indented() {
        assert!(matches!(
            marker("        // ferroforge:begin control"),
            Some(Ok(Marker::Begin("control")))
        ));
        assert!(matches!(
            marker("//ferroforge:end control"),
            Some(Ok(Marker::End("control")))
        ));
        assert!(marker("// ferroforge:beginning").is_none());
        assert!(marker("let x = 1; // ferroforge:begin x").is_none());
        assert!(matches!(marker("// ferroforge:begin"), Some(Err(_))));
    }

    #[test]
    fn notes_and_layout_are_not_code() {
        assert_eq!(normalized("    let x = 1;"), Some("let x = 1;".to_owned()));
        assert_eq!(normalized("   // a note"), None);
        assert_eq!(normalized(""), None);
        assert_eq!(normalized("/// docs"), Some("/// docs".to_owned()));
    }

    #[test]
    fn a_diff_shows_only_what_changed() {
        let before = lines(&["a", "b", "c"]);
        let after = lines(&["a", "x", "c", "d"]);
        let changes = diff(&before, &after);
        let removed = changes
            .iter()
            .filter_map(|change| match change {
                Change::Removed(line) => Some(line.text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        let added = changes
            .iter()
            .filter_map(|change| match change {
                Change::Added(line) => Some(line.text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(removed, ["b"]);
        assert_eq!(added, ["x", "d"]);
    }

    #[test]
    fn a_pattern_matches_whole_words() {
        assert!(matches_pattern("TIM*", "TIM2"));
        assert!(matches_pattern("TIM*", "TIM"));
        assert!(matches_pattern("p??", "pa5"));
        assert!(!matches_pattern("p??", "pa15"));
        assert!(!matches_pattern("TIM*", "ATIM2"));
        assert!(matches_pattern("*_IRQ", "TIM2_IRQ"));
    }

    #[test]
    fn rules_forgive_only_what_they_name() {
        let rules = Rules {
            ignore_interrupt_bindings: true,
            ignore_words: vec!["USART*".to_owned(), "p??".to_owned()],
        };
        assert_eq!(
            rules.key("#[task(binds = TIM2, priority = 2)]"),
            rules.key("#[task(binds=pac::Interrupt::TIM3, priority = 2)]"),
        );
        assert_ne!(
            rules.key("#[task(binds = TIM2, priority = 2)]"),
            rules.key("#[task(binds = TIM2, priority = 3)]"),
            "only the binding is forgiven"
        );
        assert_eq!(
            rules.key("let led = gpioa.pa5.into_push_pull_output();"),
            rules.key("let led = gpioa.pb3.into_push_pull_output();"),
        );
        assert_eq!(rules.key("cx.device.USART1"), rules.key("cx.device.USART6"));
        assert_ne!(rules.key("rebinds = 1;"), rules.key("rebinds = 2;"));
        assert_eq!(Rules::default().key("binds = TIM2"), "binds = TIM2");
    }
}
