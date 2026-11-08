//! 产物清单与人工修改检测（F-002 / [ADR-0003](../../docs/architecture/adr/accepted/ADR-0003-artifact-ownership-and-no-silent-overwrite.zh-CN.md)）。
//!
//! 产物按归属分三类，**人工修改过的生成物不被静默覆盖**：
//!
//! - **Generated**：完全由规格生成；重建时覆盖——但若检测到磁盘内容与清单记录不符
//!   （说明人工改过），则**恢复人工版本**（emitter 在内部写盘，无法前置拦截 →
//!   采用「构建前快照、构建后恢复」），并产出 `artifact-modified` 警告。
//! - **Extension**：人工实现，Forma 只保证锚点存在，**永不覆盖**（当前由 `--adopt` 产生）。
//! - **Adopted**：既有代码纳入管理（`--adopt` 登记），检查差异但**永不覆盖**。
//!
//! 冲突裁决路径（ADR-0003 §2）：`--adopt`（登记为 Adopted，人工版本胜出）、
//! `--overwrite`（显式覆盖 + 审计）、或人工改回与清单一致。
//! 冲突时构建**成功但带 warning**（`--strict` 提升为 error）。
//!
//! 清单本身是**运行时状态**（`.forma/artifacts.json`，gitignored），非产物。

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// 产物归属（ADR-0003 三分类）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Ownership {
    /// 完全由规格生成（构建可覆盖，除非检测到人工修改）
    Generated,
    /// 人工实现 / 经 `--adopt` 登记——**永不覆盖**
    Adopted,
    /// 扩展点锚点文件（ADR-0003 §6.3：扩展点文件升级不动）——构建登记、**永不覆盖**
    Extension,
}

impl Ownership {
    pub fn as_str(self) -> &'static str {
        match self {
            Ownership::Generated => "generated",
            Ownership::Adopted => "adopted",
            Ownership::Extension => "extension",
        }
    }
}

/// 产物来源规格（F-002 余项）：供缓存失效、追溯与外部工具消费。
///
/// 全部字段 `serde(default)`——旧清单无该字段时反序列化为空结构，不破坏兼容。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ArtifactSource {
    /// 生成器的实现版本（registry 查询，见 `generators::generator_version`）
    #[serde(default)]
    pub generator_version: String,
    /// 构建所用 IR 版本（`IR_VERSION`，spec:ir G-4）
    #[serde(default)]
    pub ir_version: String,
    /// 规格输入指纹：全部 `.fm` 源文件归一化内容的 sha256
    #[serde(default)]
    pub input_fingerprint: String,
    /// 输入依赖：`.fm` 源文件相对路径清单
    #[serde(default)]
    pub deps: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactEntry {
    /// 最近一次受管写入的 sha256（Generated = 生成内容；Adopted = 人工内容）
    pub hash: String,
    pub ownership: Ownership,
    /// 生成器 id（如 "gen-rust"）；Adopted 可为空
    #[serde(default)]
    pub generator: String,
    /// 来源规格（生成器版本 / IR 版本 / 输入指纹 / 依赖节点字段）
    #[serde(default)]
    pub source: Option<ArtifactSource>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ArtifactManifest {
    pub version: u8,
    #[serde(default)]
    pub artifacts: BTreeMap<String, ArtifactEntry>,
}

/// 读取产物清单（lenient：不存在/损坏时返回空）。
pub fn read_manifest(project_dir: &Path) -> ArtifactManifest {
    let path = project_dir.join(".forma/artifacts.json");
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(ArtifactManifest {
            version: 1,
            artifacts: BTreeMap::new(),
        })
}

/// 写回产物清单。
pub fn write_manifest(project_dir: &Path, manifest: &ArtifactManifest) -> Result<()> {
    let path = project_dir.join(".forma/artifacts.json");
    crate::util::write_text(&path, &serde_json::to_string_pretty(manifest)?)
}

/// 人工修改检测（构建**前**调用）：
/// 对清单中每个条目，若磁盘内容哈希 ≠ 记录哈希 → 说明人工改过（或被外部工具改过）→ 受保护。
/// 返回 (相对路径, 受保护内容快照, 归属)。
pub fn snapshot_protected(project_dir: &Path) -> Result<Vec<(String, String, Ownership)>> {
    let manifest = read_manifest(project_dir);
    let mut out = Vec::new();
    for (rel, entry) in &manifest.artifacts {
        let abs = project_dir.join(rel);
        if !abs.is_file() {
            continue; // 清单里有但磁盘没有 → 将被重新生成，不受保护
        }
        let content = crate::util::read_text(&abs)?;
        let disk = crate::util::sha256_hex(&content);
        // Adopted / Extension：**无条件保护**（即使磁盘与清单一致——
        // 否则 adopt / 锚点后的干净状态反而会让 emitter 覆盖人工版本，踩过）。
        // Generated：仅当磁盘内容 ≠ 清单记录（人工改过）才保护。
        if entry.ownership == Ownership::Adopted
            || entry.ownership == Ownership::Extension
            || disk != entry.hash
        {
            out.push((rel.clone(), content, entry.ownership));
        }
    }
    Ok(out)
}

/// 构建来源规格（F-002 余项）：由构建器计算并注入清单条目的 `source` 字段。
#[derive(Debug, Clone, Default)]
pub struct BuildProvenance {
    /// IR 版本（`IR_VERSION`）
    pub ir_version: String,
    /// 规格输入指纹（全部 `.fm` 源文件归一化内容 sha256）
    pub input_fingerprint: String,
    /// 输入依赖（`.fm` 源文件相对路径清单）
    pub deps: Vec<String>,
}

/// 扩展点锚点登记（ADR-0003 §6.3 的清单级证据）：
/// 给定 (相对路径, 生成器 id) 清单，磁盘上**已存在**的文件登记为 `Ownership::Extension`
/// （永不覆盖、升级不动）；不存在的锚点跳过（不凭空创建）。
///
/// 返回登记条数。
pub fn register_anchors(
    project_dir: &Path,
    anchors: &[(String, String)],
    prov: &BuildProvenance,
) -> Result<usize> {
    let mut manifest = read_manifest(project_dir);
    let mut n = 0;
    for (rel, generator) in anchors {
        let abs = project_dir.join(rel);
        if !abs.is_file() {
            continue; // 锚点由人工/外部创建，Forma 不凭空生成
        }
        let content = crate::util::read_text(&abs)?;
        manifest.artifacts.insert(
            rel.clone(),
            ArtifactEntry {
                hash: crate::util::sha256_hex(&content),
                ownership: Ownership::Extension,
                generator: generator.clone(),
                source: Some(ArtifactSource {
                    generator_version: crate::generators::generator_version(generator).to_string(),
                    ir_version: prov.ir_version.clone(),
                    input_fingerprint: prov.input_fingerprint.clone(),
                    deps: prov.deps.clone(),
                }),
            },
        );
        n += 1;
    }
    write_manifest(project_dir, &manifest).map(|()| n)
}

/// 构建后处理：恢复受保护内容（emitter 已在内部覆盖写盘）并产出诊断。
///
/// - 冲突且无裁决旗标 → 恢复人工版本 + `artifact-modified` 警告（`--strict` 下为 error）；
/// - `--adopt` → 恢复人工版本 + 清单登记 Adopted（人工版本胜出）；
/// - `--overwrite` → 保留生成版本 + 清单更新 Generated 哈希 + 审计留痕；
/// - **Extension 锚点 → 永远恢复锚点内容 + `artifact-extension-preserved` info**
///   （预期行为而非待裁决冲突，不告警）。
///
/// 返回 (诊断, 被恢复/改判的文件数)。
pub fn restore_protected(
    project_dir: &Path,
    snapshot: &[(String, String, Ownership)],
    adopt: bool,
    overwrite: bool,
    strict: bool,
    generated_files: &[crate::generators::GeneratedFile],
    prov: &BuildProvenance,
) -> Result<(Vec<crate::validate::Diagnostic>, usize)> {
    use crate::validate::Diagnostic;
    let mut diagnostics = Vec::new();
    let mut resolved = 0usize;
    let mut manifest = read_manifest(project_dir);
    let generated_by_path: BTreeMap<&str, &crate::generators::GeneratedFile> = generated_files
        .iter()
        .map(|f| (f.path.as_str(), f))
        .collect();

    for (rel, human_content, ownership) in snapshot {
        let abs = project_dir.join(rel);
        let disk = if abs.is_file() {
            crate::util::read_text(&abs)?
        } else {
            String::new()
        };
        if disk == *human_content {
            // emitter 未触及（或内容恰好一致）→ 无冲突
            continue;
        }
        resolved += 1;
        if *ownership == Ownership::Extension {
            // 锚点：emitter 若覆盖了则原样恢复（预期行为，info 级）
            crate::util::write_text(&abs, human_content)?;
            diagnostics.push(Diagnostic {
                severity: "info".into(),
                code: "artifact-extension-preserved".into(),
                message: format!(
                    "{rel} is an extension anchor — preserved as-is (never overwritten)"
                ),
                file: Some(rel.clone()),
                line: None,
                nodes: None,
            });
        } else if *ownership == Ownership::Adopted || adopt {
            // Adopted / --adopt：人工版本胜出 → 恢复 + 登记
            crate::util::write_text(&abs, human_content)?;
            let generator = manifest
                .artifacts
                .get(rel)
                .map(|e| e.generator.clone())
                .or_else(|| {
                    generated_by_path
                        .get(rel.as_str())
                        .map(|f| f.generator.clone())
                })
                .unwrap_or_default();
            manifest.artifacts.insert(
                rel.clone(),
                ArtifactEntry {
                    hash: crate::util::sha256_hex(human_content),
                    ownership: Ownership::Adopted,
                    generator: generator.clone(),
                    source: Some(ArtifactSource {
                        generator_version: crate::generators::generator_version(&generator)
                            .to_string(),
                        ir_version: prov.ir_version.clone(),
                        input_fingerprint: prov.input_fingerprint.clone(),
                        deps: prov.deps.clone(),
                    }),
                },
            );
            let severity = if strict { "error" } else { "warning" };
            diagnostics.push(Diagnostic {
                severity: severity.into(),
                code: "artifact-adopted".into(),
                message: format!(
                    "{rel} registered as adopted (human version preserved; generated content discarded)"
                ),
                file: Some(rel.clone()),
                line: None,
                nodes: None,
            });
        } else if overwrite {
            // --overwrite：生成版本胜出（保留 emitter 写入的内容）+ 审计
            let new_hash = crate::util::sha256_hex(&disk);
            let generator = generated_by_path
                .get(rel.as_str())
                .map(|f| f.generator.clone())
                .or_else(|| manifest.artifacts.get(rel).map(|e| e.generator.clone()))
                .unwrap_or_default();
            manifest.artifacts.insert(
                rel.clone(),
                ArtifactEntry {
                    hash: new_hash,
                    ownership: Ownership::Generated,
                    generator: generator.clone(),
                    source: Some(ArtifactSource {
                        generator_version: crate::generators::generator_version(&generator)
                            .to_string(),
                        ir_version: prov.ir_version.clone(),
                        input_fingerprint: prov.input_fingerprint.clone(),
                        deps: prov.deps.clone(),
                    }),
                },
            );
            crate::auditing::record_event(
                project_dir,
                &serde_json::json!({
                    "kind": "artifact-overwrite",
                    "file": rel,
                    "ts": crate::util::rfc3339_now(),
                }),
            )?;
            let severity = if strict { "error" } else { "warning" };
            diagnostics.push(Diagnostic {
                severity: severity.into(),
                code: "artifact-overwritten".into(),
                message: format!(
                    "{rel} overwritten by build (--overwrite); human modification discarded (audited)"
                ),
                file: Some(rel.clone()),
                line: None,
                nodes: None,
            });
        } else {
            // 默认：保留人工版本（恢复快照），警告待裁决
            crate::util::write_text(&abs, human_content)?;
            let severity = if strict { "error" } else { "warning" };
            diagnostics.push(Diagnostic {
                severity: severity.into(),
                code: "artifact-modified".into(),
                message: format!(
                    "{rel} was manually modified — kept the human version; resolve with `forma build --adopt` (register as adopted) or `--overwrite` (discard)"
                ),
                file: Some(rel.clone()),
                line: None,
                nodes: None,
            });
        }
    }

    // 生成的产物登记哈希（供下一次构建的冲突检测）
    for f in generated_files {
        // Adopted / Extension 文件即便被 emitter 覆盖，清单仍指向人工版本（下一次构建会再恢复）
        if manifest
            .artifacts
            .get(&f.path)
            .map(|e| e.ownership == Ownership::Adopted || e.ownership == Ownership::Extension)
            .unwrap_or(false)
        {
            continue;
        }
        manifest.artifacts.insert(
            f.path.clone(),
            ArtifactEntry {
                hash: crate::util::sha256_hex(&f.content),
                ownership: Ownership::Generated,
                generator: f.generator.clone(),
                source: Some(ArtifactSource {
                    generator_version: crate::generators::generator_version(&f.generator)
                        .to_string(),
                    ir_version: prov.ir_version.clone(),
                    input_fingerprint: prov.input_fingerprint.clone(),
                    deps: prov.deps.clone(),
                }),
            },
        );
    }
    write_manifest(project_dir, &manifest)?;
    Ok((diagnostics, resolved))
}

/// 清除清单条目（`--adopt` 后文件不再生成、或产物下线时使用；预留）。
#[allow(dead_code)]
pub fn remove_entry(project_dir: &Path, rel: &str) -> Result<()> {
    let mut manifest = read_manifest(project_dir);
    manifest.artifacts.remove(rel);
    write_manifest(project_dir, &manifest)
}

/// 诊断里出现的受保护路径集合（供构建器决定哪些文件需要快照——当前实现快照全量清单条目）。
pub fn manifest_paths(project_dir: &Path) -> Vec<PathBuf> {
    read_manifest(project_dir)
        .artifacts
        .keys()
        .map(|p| project_dir.join(p))
        .collect()
}
