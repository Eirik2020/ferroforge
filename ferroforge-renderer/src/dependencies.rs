//! Manifest-derived dependency requirements and conservative merging.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    path::PathBuf,
};

use crate::RenderError;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DependencySource {
    /// Cargo source identifiers are opaque; FerroForge retains and compares
    /// them exactly without interpreting their URL-like representation.
    Registry(String),
}

impl fmt::Display for DependencySource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Registry(source) => write!(formatter, "registry source `{source}`"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DependencyContributor {
    Manifest(PathBuf),
    System(String),
}

impl fmt::Display for DependencyContributor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Manifest(manifest) => write!(formatter, "manifest `{}`", manifest.display()),
            Self::System(selection) => write!(formatter, "system selection `{selection}`"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DependencyRequirement {
    pub name: String,
    pub package: String,
    pub source: DependencySource,
    pub version_requirement: String,
    pub default_features: bool,
    pub features: BTreeSet<String>,
    pub contributor: DependencyContributor,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MergedDependency {
    pub name: String,
    pub package: String,
    pub source: DependencySource,
    pub version_requirement: String,
    pub default_features: bool,
    pub features: BTreeSet<String>,
    pub contributors: BTreeSet<DependencyContributor>,
}

/// Merge identical package requirements and union their features.
///
/// Version requirements are compared as written. This intentionally does not
/// attempt semver compatibility analysis.
pub fn merge_dependencies(
    requirements: impl IntoIterator<Item = DependencyRequirement>,
) -> Result<BTreeMap<String, MergedDependency>, RenderError> {
    let mut merged = BTreeMap::<String, MergedDependency>::new();
    for requirement in requirements {
        if requirement.name != requirement.package {
            return Err(invalid(format!(
                "renamed dependency `{}` for package `{}` is not supported yet ({})",
                requirement.name, requirement.package, requirement.contributor
            )));
        }
        if let Some(existing) = merged.get_mut(&requirement.package) {
            require_equal(
                &requirement,
                existing,
                "source",
                &requirement.source,
                &existing.source,
            )?;
            require_equal(
                &requirement,
                existing,
                "version requirement",
                &requirement.version_requirement,
                &existing.version_requirement,
            )?;
            require_equal(
                &requirement,
                existing,
                "default-features",
                &requirement.default_features,
                &existing.default_features,
            )?;
            existing.features.extend(requirement.features);
            existing.contributors.insert(requirement.contributor);
        } else {
            let contributor = requirement.contributor;
            merged.insert(
                requirement.package.clone(),
                MergedDependency {
                    name: requirement.name,
                    package: requirement.package,
                    source: requirement.source,
                    version_requirement: requirement.version_requirement,
                    default_features: requirement.default_features,
                    features: requirement.features,
                    contributors: BTreeSet::from([contributor]),
                },
            );
        }
    }
    Ok(merged)
}

fn require_equal<T: fmt::Display + PartialEq>(
    incoming: &DependencyRequirement,
    existing: &MergedDependency,
    property: &str,
    incoming_value: &T,
    existing_value: &T,
) -> Result<(), RenderError> {
    if incoming_value == existing_value {
        return Ok(());
    }
    let existing_contributors = existing
        .contributors
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    Err(invalid(format!(
        "dependency `{}` has conflicting {property}: {existing_contributors} requires `{existing_value}`, but {} requires `{incoming_value}`",
        incoming.package, incoming.contributor
    )))
}

fn invalid(message: impl Into<String>) -> RenderError {
    RenderError::InvalidDeclaration(message.into())
}
