//! Read-only discovery for the new standalone source contract.
//!
//! This API does not render firmware, resolve Cargo packages, or expand macros.
//! Callers supply an explicit crate root. Module identity and original syntax
//! are retained so a later renderer can preserve scope without copying support
//! independently for each selected task instance.

use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use ferroforge_contracts::{InitContract, TaskArguments, TaskContract, identifier_key};
use serde::Deserialize;
use serde_json::Value;
use syn::{Attribute, Item, ItemFn, Meta, spanned::Spanned, visit_mut::VisitMut};

use crate::RenderError;
use crate::dependencies::{DependencyContributor, DependencyRequirement, DependencySource};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ModuleId {
    /// Canonical source root identity, not a resolved Cargo package ID.
    pub crate_root: PathBuf,
    pub path: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DefinitionId {
    pub module: ModuleId,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct TaskSource {
    pub id: DefinitionId,
    pub contract: TaskContract,
    /// Original function AST, including attributes and the complete body.
    pub function: ItemFn,
}

#[derive(Clone, Debug)]
pub struct InitSource {
    pub module: ModuleId,
    pub contract: InitContract,
    /// Original function AST, including the marker and complete native body.
    pub function: ItemFn,
}

#[derive(Clone, Debug)]
pub struct ModuleSource {
    pub id: ModuleId,
    pub file: PathBuf,
    pub attributes: Vec<Attribute>,
    /// Kept once per logical module; imports, helper functions, types, impls,
    /// constants, and statics are not pruned according to task selection.
    pub support: Vec<Item>,
    pub tasks: Vec<TaskSource>,
    pub inits: Vec<InitSource>,
    pub children: Vec<ModuleId>,
    pub visibility: syn::Visibility,
}

#[derive(Clone, Debug)]
pub struct TaskSources {
    pub root: ModuleId,
    pub modules: BTreeMap<ModuleId, ModuleSource>,
    /// Exact source text is retained alongside parsed ASTs for diagnostics and
    /// future source mapping. AST printing alone would lose comments/spacing.
    pub files: BTreeMap<PathBuf, String>,
}

/// A Cargo package containing reusable task source.
///
/// Package, target, and dependency identity come from Cargo itself.
#[derive(Clone, Debug)]
pub struct TaskPackage {
    pub name: String,
    pub package_id: String,
    pub root: PathBuf,
    pub manifest: PathBuf,
    pub library_target: String,
    pub library_root: PathBuf,
    pub dependencies: Vec<DependencyRequirement>,
    pub check_only_dependencies: BTreeSet<String>,
    pub sources: TaskSources,
}

/// A Cargo package containing exactly one standalone system init declaration.
#[derive(Clone, Debug)]
pub struct InitPackage {
    pub package: TaskPackage,
    pub init: InitSource,
}

#[derive(Clone, Debug)]
pub struct TaskInstance<'a> {
    pub name: String,
    pub definition: &'a TaskSource,
    pub support_module: &'a ModuleSource,
}

impl TaskSources {
    /// Select definitions individually. Instances from the same source module
    /// borrow the same support object; this does not yet emit RTIC namespaces.
    pub fn select<'a>(
        &'a self,
        selections: &[(&str, DefinitionId)],
    ) -> Result<Vec<TaskInstance<'a>>, RenderError> {
        let mut names = BTreeSet::new();
        selections
            .iter()
            .map(|(name, definition)| {
                let identifier = syn::parse_str::<syn::Ident>(name)
                    .map_err(|_| invalid(format!("invalid task instance name `{name}`")))?;
                if !names.insert(identifier_key(&identifier)) {
                    return Err(invalid(format!("duplicate task instance `{name}`")));
                }
                let module = self
                    .modules
                    .get(&definition.module)
                    .ok_or_else(|| invalid(format!("unknown source module for `{name}`")))?;
                let task = module
                    .tasks
                    .iter()
                    .find(|task| task.id == *definition)
                    .ok_or_else(|| {
                        invalid(format!("unknown task definition `{}`", definition.name))
                    })?;
                Ok(TaskInstance {
                    name: (*name).to_owned(),
                    definition: task,
                    support_module: module,
                })
            })
            .collect()
    }
}

/// Discover standard `mod name;` / `mod name { ... }` declarations starting at
/// an explicitly selected crate root. All files must stay within its directory.
/// Conditional source, custom module paths, task-attribute aliases, and item
/// macro expansion are outside this initial API's scope.
pub fn discover_tasks(crate_root: &Path) -> Result<TaskSources, RenderError> {
    let crate_root = crate_root.canonicalize()?;
    let source_dir = crate_root
        .parent()
        .ok_or_else(|| invalid("source root has no parent"))?
        .to_path_buf();
    let root = ModuleId {
        crate_root: crate_root.clone(),
        path: Vec::new(),
    };
    let mut sources = TaskSources {
        root: root.clone(),
        modules: BTreeMap::new(),
        files: BTreeMap::new(),
    };
    let (attrs, items) = sources.read_file(&crate_root, &source_dir)?;
    sources.walk(
        root,
        crate_root,
        source_dir.clone(),
        attrs,
        items,
        syn::Visibility::Inherited,
        &source_dir,
    )?;
    Ok(sources)
}

/// Resolve one package and its library root through `cargo metadata`, then
/// discover task modules from that root without compiling or linking it.
pub fn discover_task_package(manifest: &Path) -> Result<TaskPackage, RenderError> {
    let manifest = manifest.canonicalize()?;
    let package_root = manifest
        .parent()
        .ok_or_else(|| invalid("Cargo manifest has no parent directory"))?
        .to_path_buf();
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(cargo)
        .args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--manifest-path",
        ])
        .arg(&manifest)
        .output()?;
    if !output.status.success() {
        return Err(RenderError::CargoMetadata(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ));
    }
    let metadata: CargoMetadata = serde_json::from_slice(&output.stdout)
        .map_err(|error| RenderError::CargoMetadata(error.to_string()))?;

    let package = metadata
        .packages
        .iter()
        .find(|package| {
            package
                .manifest_path
                .canonicalize()
                .is_ok_and(|path| path == manifest)
        })
        .ok_or_else(|| {
            invalid(format!(
                "manifest does not describe a Cargo package: {}",
                manifest.display()
            ))
        })?;

    let mut libraries = package
        .targets
        .iter()
        .filter(|target| target.kind.iter().any(|kind| kind == "lib"));
    let library = libraries.next().ok_or_else(|| {
        invalid(format!(
            "task package `{}` needs a library target",
            package.name
        ))
    })?;
    if libraries.next().is_some() {
        return Err(invalid(format!(
            "task package `{}` has more than one library target",
            package.name
        )));
    }

    let library_root = library.src_path.canonicalize()?;
    if !library_root.starts_with(&package_root) {
        return Err(invalid(format!(
            "library root escapes task package `{}`: {}",
            package.name,
            library_root.display()
        )));
    }
    let (dependencies, check_only_dependencies) = collect_dependencies(package, &manifest)?;
    let sources = discover_tasks(&library_root)?;

    Ok(TaskPackage {
        name: package.name.to_string(),
        package_id: package.id.to_string(),
        root: package_root,
        manifest,
        library_target: library.name.clone(),
        library_root,
        dependencies,
        check_only_dependencies,
        sources,
    })
}

/// Discover a package containing exactly one `#[ferroforge::init]` function.
pub fn discover_init_package(manifest: &Path) -> Result<InitPackage, RenderError> {
    let package = discover_task_package(manifest)?;
    let mut declarations = package
        .sources
        .modules
        .values()
        .flat_map(|module| module.inits.iter());
    let init = declarations.next().cloned().ok_or_else(|| {
        invalid(format!(
            "init package `{}` has no `#[ferroforge::init]` declaration",
            package.name
        ))
    })?;
    if declarations.next().is_some() {
        return Err(invalid(format!(
            "init package `{}` has more than one `#[ferroforge::init]` declaration",
            package.name
        )));
    }
    Ok(InitPackage { package, init })
}

#[derive(Deserialize)]
struct CargoMetadata {
    packages: Vec<CargoPackage>,
}

#[derive(Deserialize)]
struct CargoPackage {
    name: String,
    id: String,
    manifest_path: PathBuf,
    dependencies: Vec<CargoDependency>,
    metadata: Value,
    targets: Vec<CargoTarget>,
}

#[derive(Deserialize)]
struct CargoDependency {
    name: String,
    source: Option<String>,
    kind: Option<String>,
    rename: Option<String>,
    optional: bool,
    uses_default_features: bool,
    features: Vec<String>,
    target: Option<String>,
    path: Option<PathBuf>,
}

#[derive(Deserialize)]
struct CargoTarget {
    name: String,
    kind: Vec<String>,
    src_path: PathBuf,
}

fn collect_dependencies(
    package: &CargoPackage,
    manifest: &Path,
) -> Result<(Vec<DependencyRequirement>, BTreeSet<String>), RenderError> {
    let check_only = check_only_dependencies(&package.metadata, &package.dependencies, manifest)?;
    let manifest_text = fs::read_to_string(manifest)?;
    let manifest_value: toml::Value = toml::from_str(&manifest_text).map_err(|error| {
        invalid(format!(
            "could not parse task package manifest {}: {error}",
            manifest.display()
        ))
    })?;
    let mut requirements = Vec::new();
    for dependency in package
        .dependencies
        .iter()
        .filter(|dependency| dependency.kind.is_none())
    {
        let name = dependency
            .rename
            .as_deref()
            .unwrap_or(&dependency.name)
            .to_owned();
        if check_only.contains(&name) {
            continue;
        }
        if dependency.rename.is_some() {
            return Err(unsupported_dependency(manifest, &name, "renamed"));
        }
        if dependency.optional {
            return Err(unsupported_dependency(manifest, &name, "optional"));
        }
        if let Some(target) = &dependency.target {
            return Err(unsupported_dependency(
                manifest,
                &name,
                &format!("target-specific (`{target}`)"),
            ));
        }
        if dependency.path.is_some() {
            return Err(unsupported_dependency(manifest, &name, "path"));
        }
        let source = match dependency.source.as_deref() {
            Some(source) if source.starts_with("registry+") || source.starts_with("sparse+") => {
                DependencySource::Registry(source.to_owned())
            }
            Some(source) if source.starts_with("git+") => {
                return Err(unsupported_dependency(manifest, &name, "git"));
            }
            Some(source) => {
                return Err(unsupported_dependency(
                    manifest,
                    &name,
                    &format!("source `{source}`"),
                ));
            }
            None => {
                return Err(unsupported_dependency(
                    manifest,
                    &name,
                    "dependency with no registry or path source",
                ));
            }
        };
        let version_requirement = manifest_version_requirement(&manifest_value, manifest, &name)?;
        requirements.push(DependencyRequirement {
            name,
            package: dependency.name.clone(),
            source,
            version_requirement,
            default_features: dependency.uses_default_features,
            features: dependency.features.iter().cloned().collect(),
            contributor: DependencyContributor::Manifest(manifest.to_path_buf()),
        });
    }
    requirements.sort_by(|left, right| left.name.cmp(&right.name));
    Ok((requirements, check_only))
}

fn manifest_version_requirement(
    manifest_value: &toml::Value,
    manifest: &Path,
    name: &str,
) -> Result<String, RenderError> {
    let dependencies = manifest_value
        .get("dependencies")
        .and_then(toml::Value::as_table)
        .ok_or_else(|| {
            invalid(format!(
                "{}: Cargo metadata reports normal dependency `{name}`, but [dependencies] is missing",
                manifest.display()
            ))
        })?;
    let dependency = dependencies.get(name).ok_or_else(|| {
        invalid(format!(
            "{}: Cargo metadata reports normal dependency `{name}`, but its manifest entry is missing",
            manifest.display()
        ))
    })?;
    match dependency {
        toml::Value::String(version) => Ok(version.clone()),
        toml::Value::Table(settings) => {
            if settings
                .get("workspace")
                .and_then(toml::Value::as_bool)
                .unwrap_or(false)
            {
                return Err(unsupported_dependency(
                    manifest,
                    name,
                    "workspace-inherited",
                ));
            }
            settings
                .get("version")
                .and_then(toml::Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| {
                    invalid(format!(
                        "{}: registry dependency `{name}` needs an explicit version requirement",
                        manifest.display()
                    ))
                })
        }
        _ => Err(invalid(format!(
            "{}: dependency `{name}` must use a string or table manifest entry",
            manifest.display()
        ))),
    }
}

fn check_only_dependencies(
    metadata: &Value,
    dependencies: &[CargoDependency],
    manifest: &Path,
) -> Result<BTreeSet<String>, RenderError> {
    let Some(ferroforge) = metadata.get("ferroforge") else {
        return Ok(BTreeSet::new());
    };
    let table = ferroforge.as_object().ok_or_else(|| {
        invalid(format!(
            "{}: package.metadata.ferroforge must be a table",
            manifest.display()
        ))
    })?;
    if let Some(key) = table
        .keys()
        .find(|key| key.as_str() != "check-only-dependencies")
    {
        return Err(invalid(format!(
            "{}: unknown package.metadata.ferroforge key `{key}`",
            manifest.display()
        )));
    }
    let Some(entries) = table.get("check-only-dependencies") else {
        return Ok(BTreeSet::new());
    };
    let entries = entries.as_array().ok_or_else(|| {
        invalid(format!(
            "{}: check-only-dependencies must be an array of dependency names",
            manifest.display()
        ))
    })?;
    let normal_names = dependencies
        .iter()
        .filter(|dependency| dependency.kind.is_none())
        .map(|dependency| {
            dependency
                .rename
                .as_deref()
                .unwrap_or(&dependency.name)
                .to_owned()
        })
        .collect::<BTreeSet<_>>();
    let mut check_only = BTreeSet::new();
    for entry in entries {
        let name = entry.as_str().ok_or_else(|| {
            invalid(format!(
                "{}: check-only-dependencies must contain only strings",
                manifest.display()
            ))
        })?;
        if !check_only.insert(name.to_owned()) {
            return Err(invalid(format!(
                "{}: duplicate check-only dependency `{name}`",
                manifest.display()
            )));
        }
        if !normal_names.contains(name) {
            return Err(invalid(format!(
                "{}: check-only dependency `{name}` is not a normal dependency",
                manifest.display()
            )));
        }
    }
    Ok(check_only)
}

fn unsupported_dependency(manifest: &Path, name: &str, kind: &str) -> RenderError {
    invalid(format!(
        "{}: {kind} runtime dependency `{name}` is not supported by standalone manifest collection yet",
        manifest.display()
    ))
}

impl TaskSources {
    fn read_file(
        &mut self,
        file: &Path,
        source_dir: &Path,
    ) -> Result<(Vec<Attribute>, Vec<Item>), RenderError> {
        let file = file.canonicalize()?;
        if !file.starts_with(source_dir) {
            return Err(invalid(format!(
                "source module escapes root directory: {}",
                file.display()
            )));
        }
        if self.files.contains_key(&file) {
            return Err(invalid(format!(
                "source file loaded more than once (alias or cycle): {}",
                file.display()
            )));
        }
        let text = fs::read_to_string(&file)?;
        let mut parsed = syn::parse_file(&text).map_err(|error| at_file(&file, error))?;
        let mut scope = SupportedScope::default();
        scope.visit_file_mut(&mut parsed);
        if let Some(error) = scope.error {
            return Err(at_file(&file, error));
        }
        self.files.insert(file, text);
        Ok((parsed.attrs, parsed.items))
    }

    #[allow(clippy::too_many_arguments)]
    fn walk(
        &mut self,
        id: ModuleId,
        file: PathBuf,
        module_dir: PathBuf,
        attributes: Vec<Attribute>,
        items: Vec<Item>,
        visibility: syn::Visibility,
        source_dir: &Path,
    ) -> Result<(), RenderError> {
        let mut module = ModuleSource {
            id: id.clone(),
            file: file.clone(),
            attributes,
            support: Vec::new(),
            tasks: Vec::new(),
            inits: Vec::new(),
            children: Vec::new(),
            visibility,
        };
        let mut task_names = BTreeSet::new();
        let mut child_names = BTreeSet::new();
        for item in items {
            match item {
                Item::Mod(child) => {
                    let name = identifier_key(&child.ident);
                    if !child_names.insert(name.clone()) {
                        return Err(invalid(format!(
                            "{}: duplicate module `{name}`",
                            file.display()
                        )));
                    }
                    let mut child_id = id.clone();
                    child_id.path.push(name.clone());
                    let child_dir = module_dir.join(&name);
                    if let Some((_, items)) = child.content {
                        self.walk(
                            child_id.clone(),
                            file.clone(),
                            child_dir,
                            child.attrs,
                            items,
                            child.vis,
                            source_dir,
                        )?;
                    } else {
                        let direct = module_dir.join(format!("{name}.rs"));
                        let nested = child_dir.join("mod.rs");
                        let path = match (direct.is_file(), nested.is_file()) {
                            (true, false) => direct,
                            (false, true) => nested,
                            (true, true) => {
                                return Err(invalid(format!(
                                    "{}: ambiguous module `{name}` (both .rs and mod.rs exist)",
                                    file.display()
                                )));
                            }
                            (false, false) => {
                                return Err(invalid(format!(
                                    "{}: missing source for module `{name}`",
                                    file.display()
                                )));
                            }
                        };
                        let (inner_attrs, items) = self.read_file(&path, source_dir)?;
                        let mut attrs = child.attrs;
                        attrs.extend(inner_attrs);
                        self.walk(
                            child_id.clone(),
                            path.canonicalize()?,
                            child_dir,
                            attrs,
                            items,
                            child.vis,
                            source_dir,
                        )?;
                    }
                    module.children.push(child_id);
                }
                Item::Fn(function) => {
                    let task_attributes: Vec<_> =
                        function.attrs.iter().filter(|attr| is_task(attr)).collect();
                    let init_attributes: Vec<_> =
                        function.attrs.iter().filter(|attr| is_init(attr)).collect();
                    if !task_attributes.is_empty() && !init_attributes.is_empty() {
                        return Err(invalid(format!(
                            "{}: function cannot be both a task and system init",
                            file.display()
                        )));
                    }
                    if !init_attributes.is_empty() {
                        if init_attributes.len() != 1 {
                            return Err(invalid(format!(
                                "{}: init must have exactly one init attribute",
                                file.display()
                            )));
                        }
                        if !matches!(init_attributes[0].meta, Meta::Path(_)) {
                            return Err(invalid("init attribute must be a marker"));
                        }
                        let contract = InitContract::new(&function.sig)
                            .map_err(|error| at_file(&file, error))?;
                        module.inits.push(InitSource {
                            module: id.clone(),
                            contract,
                            function,
                        });
                        continue;
                    }
                    if task_attributes.is_empty() {
                        module.support.push(Item::Fn(function));
                        continue;
                    }
                    if task_attributes.len() != 1 {
                        return Err(invalid(format!(
                            "{}: task must have exactly one task attribute",
                            file.display()
                        )));
                    }
                    let arguments: TaskArguments = match &task_attributes[0].meta {
                        Meta::Path(_) => TaskArguments::default(),
                        Meta::List(_) => task_attributes[0]
                            .parse_args()
                            .map_err(|e| at_file(&file, e))?,
                        Meta::NameValue(_) => {
                            return Err(invalid(
                                "task attribute must be a marker or an argument list",
                            ));
                        }
                    };
                    let contract = TaskContract::new(arguments, &function.sig)
                        .map_err(|e| at_file(&file, e))?;
                    let name = identifier_key(&function.sig.ident);
                    if !task_names.insert(name.clone()) {
                        return Err(invalid(format!(
                            "{}: duplicate task `{name}`",
                            file.display()
                        )));
                    }
                    module.tasks.push(TaskSource {
                        id: DefinitionId {
                            module: id.clone(),
                            name,
                        },
                        contract,
                        function,
                    });
                }
                other => module.support.push(other),
            }
        }
        if let Some(name) = task_names.intersection(&child_names).next() {
            return Err(invalid(format!(
                "{}: task context conflicts with module `{name}`",
                file.display()
            )));
        }
        self.modules.insert(id, module);
        Ok(())
    }
}

fn is_task(attribute: &Attribute) -> bool {
    let path = attribute.path();
    path.is_ident("task")
        || (path.segments.len() == 2
            && path.segments[0].ident == "ferroforge"
            && path.segments[1].ident == "task")
}

fn is_init(attribute: &Attribute) -> bool {
    let path = attribute.path();
    path.is_ident("init")
        || (path.segments.len() == 2
            && path.segments[0].ident == "ferroforge"
            && path.segments[1].ident == "init")
}

#[derive(Default)]
struct SupportedScope {
    error: Option<syn::Error>,
}

impl VisitMut for SupportedScope {
    fn visit_attribute_mut(&mut self, attribute: &mut Attribute) {
        if ["cfg", "cfg_attr", "path"]
            .iter()
            .any(|name| attribute.path().is_ident(name))
        {
            self.error.get_or_insert_with(|| syn::Error::new(attribute.span(),
                "conditional source and custom module paths are not supported by standalone discovery yet"));
        } else if !is_task(attribute)
            && !is_init(attribute)
            && ![
                "doc",
                "allow",
                "warn",
                "deny",
                "forbid",
                "deprecated",
                "inline",
                "cold",
                "must_use",
                "no_std",
                "derive",
                "repr",
            ]
            .iter()
            .any(|name| attribute.path().is_ident(name))
        {
            self.error.get_or_insert_with(|| syn::Error::new(attribute.span(),
                "unsupported attribute in standalone source; attribute macro expansion and aliases are not resolved"));
        }
    }

    fn visit_item_macro_mut(&mut self, item: &mut syn::ItemMacro) {
        self.error.get_or_insert_with(|| {
            syn::Error::new(
                item.span(),
                "item macro expansion is not supported by standalone discovery yet",
            )
        });
    }

    fn visit_item_use_mut(&mut self, item: &mut syn::ItemUse) {
        // Explicitly reject task-attribute aliasing rather than quietly treating
        // an aliased task as an ordinary helper. Other imports stay untouched.
        fn check(tree: &syn::UseTree, prefix: &mut Vec<String>) -> bool {
            match tree {
                syn::UseTree::Path(path) => {
                    prefix.push(path.ident.to_string());
                    let found = check(&path.tree, prefix);
                    prefix.pop();
                    found
                }
                syn::UseTree::Group(group) => group.items.iter().any(|tree| check(tree, prefix)),
                syn::UseTree::Rename(rename) => {
                    (prefix.is_empty() && rename.ident == "ferroforge")
                        || (prefix == &["ferroforge".to_owned()]
                            && (((rename.ident == "task" || rename.ident == "init")
                                && rename.rename != rename.ident)
                                || rename.ident == "self"))
                }
                _ => false,
            }
        }
        if check(&item.tree, &mut Vec::new()) {
            self.error.get_or_insert_with(|| {
                syn::Error::new(
                    item.span(),
                    "task/init attribute and crate aliases are not supported by standalone discovery yet",
                )
            });
        }
    }
}

fn invalid(message: impl Into<String>) -> RenderError {
    RenderError::InvalidDeclaration(message.into())
}

fn at_file(file: &Path, error: syn::Error) -> RenderError {
    invalid(format!("{}: {error}", file.display()))
}
