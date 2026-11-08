use anyhow::Context;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// 原子写临时文件的进程内序号（并行构建 / 并行测试不重名）。
static WRITE_SEQ: AtomicU64 = AtomicU64::new(0);

pub fn read_text(path: &Path) -> anyhow::Result<String> {
    fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))
}

/// RFC-0007 §16：**内容哈希跳过写入 + 原子写**。
///
/// - 磁盘内容经 `sha256_hex`（换行归一化口径）比较未变 → 返回 `false`，零写入；
///   连续两次构建不触碰产物文件（mtime 不变），人工修改也不会被无谓覆盖。
/// - 写入走「同目录临时文件 → `fs::rename`」：读者只可能看到旧文件或完整新文件，
///   不会看到半截内容。临时文件名带 pid + 纳秒 + 进程内序号，避免并行竞态。
///
/// 返回是否真的写了磁盘（供回归测试与构建报告消费）。
pub fn write_text_changed(path: &Path, content: &str) -> anyhow::Result<bool> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create dir {}", parent.display()))?;
    }
    if let Ok(old) = fs::read_to_string(path) {
        if crate::util::sha256_hex(&old) == crate::util::sha256_hex(content) {
            return Ok(false);
        }
    }
    let seq = WRITE_SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "artifact".into());
    let tmp = path.with_file_name(format!(".{name}.tmp-{}-{nanos}-{seq}", std::process::id()));
    {
        let mut f =
            fs::File::create(&tmp).with_context(|| format!("cannot write {}", tmp.display()))?;
        f.write_all(content.as_bytes())?;
        f.sync_all()?;
    }
    // POSIX rename 为原子替换；CI 运行于 ubuntu / macos。
    fs::rename(&tmp, path).with_context(|| format!("cannot rename into {}", path.display()))?;
    Ok(true)
}

pub fn write_text(path: &Path, content: &str) -> anyhow::Result<()> {
    write_text_changed(path, content).map(|_| ())
}

pub fn exists(path: &Path) -> bool {
    path.exists()
}

/// 产物哈希的统一口径（spec:serialization §10）：
/// - **换行归一化**（P1-4）：`\r\n` / `\r` → `\n`——Windows 检出的工作树
///   （`core.autocrlf`）不得对同一产物产生假差异；
/// - UTF-8（无 BOM）→ SHA-256。
///
/// **口径决议（P1-5）**：Forma 的产物**全部是文本**（生成器只产文本文件），
/// 故不设二进制哈希口径；若未来引入二进制产物，**必须**新增 `sha256_bytes`
/// 并在本文件与 spec:serialization 声明，**不得**复用本函数。
pub fn sha256_hex(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(normalize_newlines(text).as_bytes());
    let out = hasher.finalize();
    out.iter().map(|b| format!("{:02x}", b)).collect()
}

/// 行尾归一化：CRLF / CR → LF。
fn normalize_newlines(text: &str) -> String {
    if !text.contains('\r') {
        return text.to_string();
    }
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// 稳定 JSON 序列化（对象键排序）——用于哈希结构化数据。
pub fn stable_stringify(value: &serde_json::Value) -> String {
    format!("{}", sort_value(value))
}

fn sort_value(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(sort_value).collect())
        }
        serde_json::Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let mut out = serde_json::Map::new();
            for k in keys {
                out.insert(k.clone(), sort_value(&map[k]));
            }
            serde_json::Value::Object(out)
        }
        other => other.clone(),
    }
}

pub fn hash_value(value: &serde_json::Value) -> String {
    sha256_hex(&stable_stringify(value))
}

/// 字符偏移 → 1-based 行号。
pub fn line_of(text: &str, offset: usize) -> usize {
    text[..offset.min(text.len())]
        .bytes()
        .filter(|b| *b == b'\n')
        .count()
        + 1
}

/// 项目相对路径（POSIX 风格）。
pub fn rel_path(base: &Path, full: &Path) -> String {
    full.strip_prefix(base)
        .unwrap_or(full)
        .to_string_lossy()
        .replace('\\', "/")
}

pub fn pathbuf(s: &str) -> PathBuf {
    PathBuf::from(s)
}

/// 当前时间的 RFC3339 UTC 字符串（秒精度）。
///
/// 审计记录是运行时状态（`.forma/` gitignored），非产物——不参与确定性约束；
/// 保持零新增依赖， civil-from-days 算法手写（Howard Hinnant 版）。
pub fn rfc3339_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // civil_from_days（Hinnant）
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };
    format!("{year:04}-{month:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_ignores_line_ending_differences() {
        // P1-4：Windows（CRLF）与 Unix（LF）检出的同一产物必须同哈希
        let lf = "line1\nline2\n";
        let crlf = "line1\r\nline2\r\n";
        assert_eq!(sha256_hex(lf), sha256_hex(crlf));
        // 孤立 CR 也归一化
        assert_eq!(sha256_hex(lf), sha256_hex("line1\rline2\r"));
    }

    #[test]
    fn hash_still_distinguishes_content() {
        assert_ne!(sha256_hex("a"), sha256_hex("b"));
    }

    #[test]
    fn hash_value_is_key_order_stable() {
        use serde_json::json;
        let a = json!({ "a": 1, "b": 2 });
        let b = json!({ "b": 2, "a": 1 });
        assert_eq!(hash_value(&a), hash_value(&b));
    }
}
