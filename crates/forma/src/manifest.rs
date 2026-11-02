//! forma.yaml Manifest：声明规格清单、标准版本、身份映射与编译目标。不含业务语义。

use anyhow::Context;
use serde::Serialize;
use std::path::{Path, PathBuf};

pub const MANIFEST_FILE: &str = "forma.yaml";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpecFormat {
    Sql,
    JsonSchema,
    OpenApi,
    JsonForms,
    Bpmn,
}

impl SpecFormat {
    pub fn as_str(&self) -> &'static str {
        match self {
            SpecFormat::Sql => "sql",
            SpecFormat::JsonSchema => "json-schema",
            SpecFormat::OpenApi => "openapi",
            SpecFormat::JsonForms => "json-forms",
            SpecFormat::Bpmn => "bpmn",
        }
    }
}

/// Manifest 分组名（领域导向）→ 规格格式 + 加载顺序。
/// 用户写 database/schemas/api/ui/workflows；格式名也接受作为别名。
pub fn spec_group(group: &str) -> Option<(&'static str, SpecFormat, u8)> {
    match group {
        "schemas" | "json-schema" => Some(("schemas", SpecFormat::JsonSchema, 0)),
        "api" | "openapi" => Some(("api", SpecFormat::OpenApi, 1)),
        "ui" | "json-forms" => Some(("ui", SpecFormat::JsonForms, 2)),
        "database" | "sql" => Some(("database", SpecFormat::Sql, 3)),
        "workflows" | "bpmn" => Some(("workflows", SpecFormat::Bpmn, 4)),
        _ => None,
    }
}

pub const NOT_YET_SUPPORTED_GROUPS: [&str; 3] = [
    "decisions",     // DMN — Phase 1
    "events",        // AsyncAPI — Phase 1
    "architecture",  // C4/Structurizr — Phase 2
];

pub const GROUP_ORDER: [&str; 5] = ["schemas", "api", "ui", "database", "workflows"];

#[derive(Debug, Clone)]
pub struct SpecEntry {
    pub path: String,
    pub format: SpecFormat,
    pub dialect: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone)]
pub struct IdentityBinding {
    pub uri: String,
    pub table: String,
}

#[derive(Debug, Clone, Default)]
pub struct TargetConfig {
    pub framework: Option<String>,
    pub out: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Targets {
    pub typescript: Option<String>,
    pub backend: Option<TargetConfig>,
    pub database: Option<TargetConfig>,
    pub frontend: Option<TargetConfig>,
}

#[derive(Debug, Clone, Default)]
pub struct FormaManifest {
    pub project_name: String,
    pub project_version: String,
    /// 以领域分组名（schemas/api/ui/database/workflows）为键
    pub specifications: Vec<(String, Vec<SpecEntry>)>,
    pub bindings: Vec<IdentityBinding>,
    pub targets: Targets,
}

impl FormaManifest {
    pub fn group(&self, name: &str) -> &[SpecEntry] {
        for (key, entries) in &self.specifications {
            if key == name {
                return entries;
            }
        }
        &[]
    }
}

#[derive(Debug, Serialize)]
pub struct ManifestIssue {
    pub severity: String,
    pub message: String,
}

pub struct LoadedManifest {
    pub manifest: FormaManifest,
    pub file: PathBuf,
    pub dir: PathBuf,
    pub issues: Vec<ManifestIssue>,
}

pub fn load_manifest(project_dir: &Path) -> anyhow::Result<LoadedManifest> {
    let file = project_dir.join(MANIFEST_FILE);
    let mut issues: Vec<ManifestIssue> = Vec::new();
    if !file.is_file() {
        anyhow::bail!("missing {MANIFEST_FILE} in {}", project_dir.display());
    }
    let raw: serde_yaml::Value =
        serde_yaml::from_str(&crate::util::read_text(&file)?).context("forma.yaml parse error")?;
    let manifest = normalize_manifest(&raw, &mut issues)?;
    check_entries(&manifest, &file.parent().unwrap().to_path_buf(), &mut issues)?;
    Ok(LoadedManifest {
        manifest,
        dir: file.parent().unwrap().to_path_buf(),
        file,
        issues,
    })
}

fn normalize_manifest(
    raw: &serde_yaml::Value,
    issues: &mut Vec<ManifestIssue>,
) -> anyhow::Result<FormaManifest> {
    let root = raw
        .as_mapping()
        .ok_or_else(|| anyhow::anyhow!("forma.yaml: root must be a mapping"))?;

    let project = root
        .get(serde_yaml::Value::String("project".into()))
        .and_then(|v| v.as_mapping())
        .ok_or_else(|| anyhow::anyhow!("forma.yaml: project.name is required"))?;
    let project_name = project
        .get(serde_yaml::Value::String("name".into()))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("forma.yaml: project.name is required"))?
        .to_string();
    let project_version = project
        .get(serde_yaml::Value::String("version".into()))
        .and_then(|v| v.as_str())
        .unwrap_or("0.0.0")
        .to_string();

    let mut specifications: Vec<(String, Vec<SpecEntry>)> = Vec::new();
    if let Some(specs) = root.get(serde_yaml::Value::String("specifications".into())).and_then(|v| v.as_mapping()) {
        for (key, value) in specs {
            let group_name = key.as_str().unwrap_or_default().to_string();
            if NOT_YET_SUPPORTED_GROUPS.contains(&group_name.as_str()) {
                issues.push(ManifestIssue {
                    severity: "warning".into(),
                    message: format!("specification group \"{group_name}\" is planned but not yet supported (ignored)"),
                });
                continue;
            }
            let Some((canonical, format, _)) = spec_group(&group_name) else {
                issues.push(ManifestIssue {
                    severity: "warning".into(),
                    message: format!("unknown specification group: {group_name}"),
                });
                continue;
            };
            let list = value.as_sequence().cloned().unwrap_or_default();
            let mut entries = Vec::new();
            for item in &list {
                if let Some(entry) = normalize_spec_entry(item, format, issues) {
                    entries.push(entry);
                }
            }
            // 同一 canonical 分组可能以别名重复出现 — 合并
            if let Some(slot) = specifications.iter_mut().find(|(k, _)| k == canonical) {
                slot.1.extend(entries);
            } else {
                specifications.push((canonical.to_string(), entries));
            }
        }
    }

    let mut bindings = Vec::new();
    if let Some(list) = root.get(serde_yaml::Value::String("bindings".into())).and_then(|v| v.as_sequence()) {
        for item in list {
            let uri = item.get("uri").and_then(|v| v.as_str()).unwrap_or_default().to_string();
            let table = item.get("table").and_then(|v| v.as_str()).unwrap_or_default().to_string();
            if uri.is_empty() || table.is_empty() {
                issues.push(ManifestIssue {
                    severity: "warning".into(),
                    message: format!("invalid binding entry: {item:?}"),
                });
            } else {
                bindings.push(IdentityBinding { uri, table });
            }
        }
    }

    let mut targets = Targets::default();
    if let Some(t) = root.get(serde_yaml::Value::String("targets".into())).and_then(|v| v.as_mapping()) {
        targets.typescript = t
            .get(serde_yaml::Value::String("typescript".into()))
            .and_then(|v| v.as_str())
            .map(String::from);
        for (name, slot) in [
            ("backend", &mut targets.backend),
            ("database", &mut targets.database),
            ("frontend", &mut targets.frontend),
        ] {
            if let Some(cfg) = t.get(serde_yaml::Value::String(name.into())).and_then(|v| v.as_mapping()) {
                *slot = Some(TargetConfig {
                    framework: cfg
                        .get(serde_yaml::Value::String("framework".into()))
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    out: cfg
                        .get(serde_yaml::Value::String("out".into()))
                        .and_then(|v| v.as_str())
                        .map(String::from),
                });
            }
        }
    }

    Ok(FormaManifest {
        project_name,
        project_version,
        specifications,
        bindings,
        targets,
    })
}

fn normalize_spec_entry(
    item: &serde_yaml::Value,
    format: SpecFormat,
    issues: &mut Vec<ManifestIssue>,
) -> Option<SpecEntry> {
    if let Some(s) = item.as_str() {
        return Some(SpecEntry {
            path: s.to_string(),
            format,
            dialect: None,
            version: None,
        });
    }
    let path = item.get("path").and_then(|v| v.as_str())?;
    if path.is_empty() {
        issues.push(ManifestIssue {
            severity: "warning".into(),
            message: format!("invalid spec entry: {item:?}"),
        });
        return None;
    }
    Some(SpecEntry {
        path: path.to_string(),
        format,
        dialect: item.get("dialect").and_then(|v| v.as_str()).map(String::from),
        version: item.get("version").and_then(|v| v.as_str()).map(String::from),
    })
}

fn check_entries(manifest: &FormaManifest, dir: &Path, issues: &mut Vec<ManifestIssue>) -> anyhow::Result<()> {
    for (group, entries) in &manifest.specifications {
        for entry in entries {
            let abs = dir.join(&entry.path);
            if !abs.is_file() {
                issues.push(ManifestIssue {
                    severity: "error".into(),
                    message: format!("specification file not found: {} ({group})", entry.path),
                });
            }
        }
    }
    for binding in &manifest.bindings {
        if !binding.uri.starts_with("forma://project/") {
            issues.push(ManifestIssue {
                severity: "warning".into(),
                message: format!("binding uri not a forma://project URI: {}", binding.uri),
            });
        }
    }
    Ok(())
}

pub fn resolve_spec_path(loaded: &LoadedManifest, rel: &str) -> PathBuf {
    loaded.dir.join(rel)
}
