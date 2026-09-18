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

use crate::{all::Style, project::Project};

const BEGIN: &str = "ferroforge:begin";
const END: &str = "ferroforge:end";

/// One marked region in one firmware.
struct Region {
    /// Path shown in reports, relative to the project.
    file: String,
    begin: usize,
    depth: usize,
    /// Normalized lines with the line number each came from.
    lines: Vec<(usize, String)>,
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
fn scan(root: &Path, firmware: &Path) -> (BTreeMap<String, Region>, Vec<String>) {
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
                    push(&mut regions, &open, number, &content);
                }
                Some(Ok(Marker::End(name))) => match open.last() {
                    Some(innermost) if innermost == name => {
                        push(&mut regions, &open, number, &content);
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
                None => push(&mut regions, &open, number, &content),
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
) {
    let Some(content) = content else { return };
    for name in open {
        if let Some(region) = regions.get_mut(name) {
            region.lines.push((number, content.clone()));
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
    Removed(&'a (usize, String)),
    Added(&'a (usize, String)),
}

/// A line diff by longest common subsequence. Regions are short, so the
/// quadratic table costs nothing worth a dependency.
fn diff<'a>(before: &'a [(usize, String)], after: &'a [(usize, String)]) -> Vec<Change<'a>> {
    let (n, m) = (before.len(), after.len());
    let mut common = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            common[i][j] = if before[i].1 == after[j].1 {
                common[i + 1][j + 1] + 1
            } else {
                common[i + 1][j].max(common[i][j + 1])
            };
        }
    }
    let (mut i, mut j, mut changes) = (0, 0, Vec::new());
    while i < n || j < m {
        if i < n && j < m && before[i].1 == after[j].1 {
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
            .all(|(a, b)| a.1 == b.1)
}

pub fn run(project: &Project) -> Result<ExitCode, String> {
    let firmwares = project.firmwares().map_err(|error| error.to_string())?;
    let style = Style::detect();

    let mut scanned = Vec::new();
    let mut errors = Vec::new();
    for firmware in &firmwares {
        let (regions, mut found) = scan(&project.root, &firmware.path);
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
                    Change::Removed((number, line)) => println!(
                        "    {}",
                        style.paint("31", &format!("- {number:>4}  {line}"))
                    ),
                    Change::Added((number, line)) => println!(
                        "    {}",
                        style.paint("32", &format!("+ {number:>4}  {line}"))
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

    fn lines(text: &[&str]) -> Vec<(usize, String)> {
        text.iter()
            .enumerate()
            .map(|(index, line)| (index + 1, (*line).to_owned()))
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
                Change::Removed((_, line)) => Some(line.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        let added = changes
            .iter()
            .filter_map(|change| match change {
                Change::Added((_, line)) => Some(line.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(removed, ["b"]);
        assert_eq!(added, ["x", "d"]);
    }
}
