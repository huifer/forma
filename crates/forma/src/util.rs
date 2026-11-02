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
    let mut f = fs::File::create(path).with_context(|| format!("cannot write {}", path.display()))?;
    f.write_all(content.as_bytes())?;
    Ok(())
}

pub fn exists(path: &Path) -> bool {
    path.exists()
}

pub fn sha256_hex(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    let out = hasher.finalize();
    out.iter().map(|b| format!("{:02x}", b)).collect()
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
    text[..offset.min(text.len())].bytes().filter(|b| *b == b'\n').count() + 1
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
