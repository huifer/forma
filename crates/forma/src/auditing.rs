//! 编译器侧审计（RFC-0006 P5 / §10）：补丁应用留痕。
//!
//! 每次补丁尝试（applied / rejected / failed）追加一行 JSON 到
//! `<project>/.forma/audit.jsonl`——记录操作集、风险等级、审批来源与产物重建规模。
//!
//! `.forma/` 是 gitignored 的运行时状态目录（与迁移台账同类）：审计是**本机追溯记录**，
//! 不参与产物确定性约束（非确定性时间戳只允许出现在这里与产物清单层，见 spec:ir §11 R3）。

use anyhow::Result;
use std::io::Write;
use std::path::Path;

/// 一条补丁审计记录。
#[derive(Debug, serde::Serialize)]
pub struct PatchAuditEvent {
    /// RFC3339 UTC（秒精度）
    pub ts: String,
    pub project: String,
    /// applied | rejected | failed
    pub status: String,
    /// 各操作的 `op` 名（按提交顺序）
    pub operations: Vec<String>,
    /// 风险评估（迁移风险 + 权限变更标记）
    pub risk: serde_json::Value,
    /// 审批来源（高风险补丁必须携带）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_by: Option<String>,
    /// 审批理由（进审计记录）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_reason: Option<String>,
    /// 实际改动的规格文件
    #[serde(default, rename = "filesChanged")]
    pub files_changed: Vec<String>,
    /// 重建产物数（rejected/failed 为 0）
    #[serde(default, rename = "buildFiles")]
    pub build_files: usize,
    /// 拒绝原因 / 失败原因
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// 追加一条审计记录（`.forma/audit.jsonl`，一行一个 JSON 对象）。
pub fn record_patch_event(project_dir: &Path, event: &PatchAuditEvent) -> Result<()> {
    record_event(project_dir, &serde_json::to_value(event)?)
}

/// 追加一条任意审计事件（产物覆盖、补丁、未来的治理动作共用同一落点）。
pub fn record_event(project_dir: &Path, event: &serde_json::Value) -> Result<()> {
    let dir = project_dir.join(".forma");
    std::fs::create_dir_all(&dir)?;
    let mut line = serde_json::to_string(event)?;
    line.push('\n');
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("audit.jsonl"))?;
    f.write_all(line.as_bytes())?;
    Ok(())
}

/// 读取最近 `n` 条审计记录（时间正序；文件不存在或损坏时返回空）。
pub fn read_recent(project_dir: &Path, n: usize) -> Vec<serde_json::Value> {
    let path = project_dir.join(".forma/audit.jsonl");
    let Ok(text) = std::fs::read_to_string(path) else {
        return vec![];
    };
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(n);
    lines[start..]
        .iter()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}
