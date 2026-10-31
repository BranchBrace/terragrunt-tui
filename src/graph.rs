use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unit {
    pub path: String,
    pub dependencies: Vec<String>,
}

#[derive(Debug)]
pub enum DiscoveryError {
    Json(serde_json::Error),
    Invalid(String),
}

impl std::fmt::Display for DiscoveryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(error) => write!(formatter, "invalid Terragrunt discovery JSON: {error}"),
            Self::Invalid(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for DiscoveryError {}

pub fn parse_units(json: &str) -> Result<Vec<Unit>, DiscoveryError> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(DiscoveryError::Json)?;
    let rows = value
        .as_array()
        .ok_or_else(|| DiscoveryError::Invalid("unit discovery must be a JSON array".into()))?;
    let mut units = Vec::new();
    for row in rows {
        let Some(kind) = row.get("type").and_then(serde_json::Value::as_str) else {
            return Err(DiscoveryError::Invalid("unit entry is missing type".into()));
        };
        if kind != "unit" {
            continue;
        }
        let path = row
            .get("path")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| DiscoveryError::Invalid("unit entry is missing path".into()))?;
        let dependencies = match row.get("dependencies") {
            None => Vec::new(),
            Some(value) => value
                .as_array()
                .ok_or_else(|| {
                    DiscoveryError::Invalid(format!("dependencies for {path} must be an array"))
                })?
                .iter()
                .map(|dependency| {
                    dependency.as_str().map(str::to_owned).ok_or_else(|| {
                        DiscoveryError::Invalid(format!("dependency for {path} is not a string"))
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
        };
        units.push(Unit {
            path: path.to_owned(),
            dependencies,
        });
    }
    if units.is_empty() {
        return Err(DiscoveryError::Invalid(
            "Terragrunt discovered no units".into(),
        ));
    }
    Ok(units)
}

pub fn dependency_waves(root: &Path, units: &[Unit]) -> BTreeMap<String, Option<usize>> {
    let paths: BTreeSet<String> = units
        .iter()
        .map(|unit| normalize(root, Path::new(&unit.path)))
        .collect();
    let mut pending = paths.clone();
    let mut waves = BTreeMap::new();
    while !pending.is_empty() {
        let mut ready = Vec::new();
        for path in &pending {
            let Some(unit) = units
                .iter()
                .find(|unit| normalize(root, Path::new(&unit.path)) == *path)
            else {
                continue;
            };
            let dependencies = unit
                .dependencies
                .iter()
                .map(|dependency| resolve_dependency(root, &unit.path, dependency, &paths))
                .collect::<Vec<_>>();
            if dependencies
                .iter()
                .any(|dependency| !paths.contains(dependency))
            {
                waves.insert(path.clone(), None);
                continue;
            }
            if dependencies
                .iter()
                .all(|dependency| waves.get(dependency).is_some_and(Option::is_some))
            {
                ready.push((path.clone(), dependencies));
            }
        }
        if ready.is_empty() {
            for path in pending {
                waves.entry(path).or_insert(None);
            }
            break;
        }
        for (path, dependencies) in ready {
            let level = dependencies
                .iter()
                .filter_map(|dependency| waves.get(dependency).copied().flatten())
                .max()
                .map_or(0, |parent| parent + 1);
            waves.insert(path.clone(), Some(level));
            pending.remove(&path);
        }
        pending.retain(|path| !waves.contains_key(path));
    }
    waves
}

fn resolve_dependency(
    root: &Path,
    unit: &str,
    dependency: &str,
    known_units: &BTreeSet<String>,
) -> String {
    let dependency_path = Path::new(dependency);
    if dependency_path.is_absolute() {
        return normalize(root, dependency_path);
    }
    let root_relative = normalize(root, &root.join(dependency_path));
    // Terragrunt discovery may emit root-relative paths or legacy unit-relative paths.
    if known_units.contains(&root_relative) {
        return root_relative;
    }
    let unit_relative = Path::new(unit);
    let unit_directory = if unit_relative.is_absolute() {
        unit_relative.to_path_buf()
    } else {
        root.join(unit_relative)
    };
    normalize(root, &unit_directory.join(dependency_path))
}

fn normalize(root: &Path, path: &Path) -> String {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_terragrunt_units_and_computes_dependency_depth_not_runtime_order() {
        let units = parse_units(
            r#"[
          {"type":"unit","path":"network","dependencies":[]},
          {"type":"unit","path":"database","dependencies":["../network"]},
          {"type":"unit","path":"service","dependencies":["../database"]},
          {"type":"unit","path":"metrics","dependencies":[]},
          {"type":"stack","path":"generated"}
        ]"#,
        )
        .unwrap();
        let waves = dependency_waves(Path::new("/fixture"), &units);
        assert_eq!(waves["/fixture/network"], Some(0));
        assert_eq!(waves["/fixture/metrics"], Some(0));
        assert_eq!(waves["/fixture/database"], Some(1));
        assert_eq!(waves["/fixture/service"], Some(2));
    }

    #[test]
    fn resolves_root_relative_dependency_paths() {
        let units = parse_units(
            r#"[
          {"type":"unit","path":"live/dev/vpc","dependencies":[]},
          {"type":"unit","path":"live/dev/app","dependencies":["live/dev/vpc"]}
        ]"#,
        )
        .unwrap();

        let waves = dependency_waves(Path::new("/fixture"), &units);

        assert_eq!(waves["/fixture/live/dev/vpc"], Some(0));
        assert_eq!(waves["/fixture/live/dev/app"], Some(1));
    }

    #[test]
    fn missing_dependency_and_cycles_are_unknown_not_guessed() {
        let units = vec![
            Unit {
                path: "service".into(),
                dependencies: vec!["../outside".into()],
            },
            Unit {
                path: "a".into(),
                dependencies: vec!["../b".into()],
            },
            Unit {
                path: "b".into(),
                dependencies: vec!["../a".into()],
            },
        ];
        let waves = dependency_waves(Path::new("/fixture"), &units);
        assert_eq!(waves["/fixture/service"], None);
        assert_eq!(waves["/fixture/a"], None);
        assert_eq!(waves["/fixture/b"], None);
    }

    #[test]
    fn rejects_malformed_discovery_data() {
        assert!(parse_units("{} ").is_err());
        assert!(parse_units(r#"[{"type":"unit"}]"#).is_err());
    }
}
