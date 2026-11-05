use anyhow::Context;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

pub fn read_text(path: &Path) -> anyhow::Result<String> {
    fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))
}

pub fn write_text(path: &Path, content: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create dir {}", parent.display()))?;
    }
    let mut f =
        fs::File::create(path).with_context(|| format!("cannot write {}", path.display()))?;
    f.write_all(content.as_bytes())?;
    Ok(())
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
