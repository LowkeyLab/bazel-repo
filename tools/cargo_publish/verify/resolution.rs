use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use cargo_publish_core::{GenerationContract, ResolvedDependency, ResolvedPackage};
use serde::Deserialize;

use crate::manifest::{Requirement, declared_requirements};

const REGISTRY: &str = "registry+https://github.com/rust-lang/crates.io-index";

#[derive(Deserialize)]
pub struct Metadata {
    packages: Vec<Package>,
    resolve: Resolve,
}
#[derive(Deserialize)]
struct Package {
    id: String,
    name: String,
    version: String,
    source: Option<String>,
    dependencies: Option<Vec<Declaration>>,
    targets: Option<Vec<Target>>,
}
#[derive(Deserialize)]
struct Declaration {
    name: String,
    rename: Option<String>,
    kind: Option<String>,
    source: Option<String>,
    req: String,
    uses_default_features: bool,
    features: Vec<String>,
    optional: bool,
    target: Option<String>,
}
#[derive(Deserialize)]
struct Target {
    name: String,
    kind: Vec<String>,
    src_path: PathBuf,
    edition: String,
}
#[derive(Deserialize)]
struct Resolve {
    root: String,
    nodes: Vec<Node>,
}
#[derive(Deserialize)]
struct Node {
    id: String,
    features: Vec<String>,
    deps: Vec<ResolvedDependency>,
}

pub fn compare_resolution(
    metadata: Metadata,
    contract: &GenerationContract,
    extracted: &Path,
) -> Result<Vec<ResolvedPackage>> {
    let packages: BTreeMap<_, _> = metadata
        .packages
        .iter()
        .map(|package| (package.id.as_str(), package))
        .collect();
    ensure!(
        packages.len() == metadata.packages.len(),
        "duplicate Cargo metadata package IDs"
    );
    let root = packages
        .get(metadata.resolve.root.as_str())
        .context("missing Cargo metadata root")?;
    ensure!(
        root.name == contract.package.name && root.version == contract.package.version,
        "Cargo metadata root package identity/version differs"
    );
    compare_declarations(root, contract)?;
    compare_targets(root, contract, extracted)?;
    let root_node = metadata
        .resolve
        .nodes
        .iter()
        .find(|node| node.id == metadata.resolve.root)
        .context("missing Cargo metadata root node")?;
    compare_direct_edges(root_node, &packages, contract)?;
    metadata
        .resolve
        .nodes
        .into_iter()
        .map(|node| {
            let package = packages
                .get(node.id.as_str())
                .context("resolved graph node has no package")?;
            let mut features = node.features;
            features.sort();
            Ok(ResolvedPackage {
                id: node.id,
                name: package.name.clone(),
                version: package.version.clone(),
                source: package.source.clone(),
                features,
                deps: node.deps,
            })
        })
        .collect()
}

fn compare_declarations(root: &Package, contract: &GenerationContract) -> Result<()> {
    let expected_declarations = declared_requirements(&contract.dependencies)?;
    let mut actual_declarations = BTreeMap::new();
    for declaration in root
        .dependencies
        .as_ref()
        .context("missing root metadata dependencies")?
    {
        let alias = declaration.rename.as_ref().unwrap_or(&declaration.name);
        let kind = declaration.kind.as_deref().unwrap_or("normal");
        ensure!(
            !declaration.optional
                && declaration.target.is_none()
                && declaration.source.as_deref() == Some(REGISTRY),
            "unsupported direct metadata declaration: {alias}"
        );
        let mut features = declaration.features.clone();
        features.sort();
        ensure!(
            actual_declarations
                .insert(
                    (alias.clone(), kind.into()),
                    Requirement {
                        package: declaration.name.clone(),
                        version: Some(declaration.req.clone()),
                        default_features: declaration.uses_default_features,
                        features
                    }
                )
                .is_none(),
            "conflicting direct metadata declaration: {alias}"
        );
    }
    ensure!(
        actual_declarations == expected_declarations,
        "Cargo metadata direct identity/alias/kind/requirement/defaults/features differ: expected {expected_declarations:?}; actual {actual_declarations:?}"
    );
    Ok(())
}

fn compare_targets(root: &Package, contract: &GenerationContract, extracted: &Path) -> Result<()> {
    let mut expected_targets = BTreeSet::new();
    if let Some(library) = &contract.selection.library {
        expected_targets.insert((
            "lib".to_string(),
            library.crate_name.clone(),
            library.path.to_string(),
            library.edition.clone(),
        ));
    }
    expected_targets.extend(contract.selection.binaries.iter().map(|binary| {
        (
            "bin".to_string(),
            binary.name.clone(),
            binary.path.to_string(),
            binary.edition.clone(),
        )
    }));
    let mut actual_targets = BTreeSet::new();
    let root_targets = root
        .targets
        .as_ref()
        .context("missing root metadata targets")?;
    for target in root_targets {
        let kind = if target.kind == ["lib"] || target.kind == ["rlib"] {
            "lib"
        } else {
            target
                .kind
                .first()
                .context("missing metadata target kind")?
        };
        let relative = target
            .src_path
            .strip_prefix(extracted)
            .context("Cargo metadata target lies outside extracted archive")?;
        actual_targets.insert((
            kind.to_string(),
            target.name.clone(),
            relative.to_string_lossy().into_owned(),
            target.edition.clone(),
        ));
    }
    ensure!(
        actual_targets.len() == root_targets.len() && actual_targets == expected_targets,
        "Cargo metadata selected targets differ: {actual_targets:?}"
    );
    Ok(())
}

fn compare_direct_edges(
    root_node: &Node,
    packages: &BTreeMap<&str, &Package>,
    contract: &GenerationContract,
) -> Result<()> {
    let expected: BTreeSet<_> = contract
        .dependencies
        .iter()
        .map(|dependency| {
            (
                dependency.alias.replace('-', "_"),
                match dependency.kind {
                    cargo_publish_core::DependencyKind::Normal => "normal".to_string(),
                    cargo_publish_core::DependencyKind::Dev => "dev".to_string(),
                },
                dependency.package.clone(),
                dependency.version.clone(),
            )
        })
        .collect();
    let mut actual = BTreeSet::new();
    for edge in &root_node.deps {
        let package = packages
            .get(edge.pkg.as_str())
            .context("missing directly resolved package")?;
        ensure!(
            package.source.as_deref() == Some(REGISTRY),
            "unsupported direct resolved source: {}",
            edge.name
        );
        for kind in &edge.dep_kinds {
            ensure!(
                kind.target.is_none(),
                "unsupported conditional direct dependency: {}",
                edge.name
            );
            actual.insert((
                edge.name.clone(),
                kind.kind.clone().unwrap_or_else(|| "normal".into()),
                package.name.clone(),
                package.version.clone(),
            ));
        }
    }
    ensure!(
        actual == expected,
        "resolved direct identity/version/alias/kind mismatch: expected {expected:?}; actual {actual:?}"
    );
    Ok(())
}
