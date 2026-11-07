//! Redis target（M9/P1）：Program → Redis **键设计派生**（缓存 / 会话 / 限流）。
//!
//! Redis 不是关系型存储——没有表 / DDL。本 emitter 从 DSL 的实体、索引、
//! 唯一约束、auth 模块、jobs、live 推导出**键设计**（key patterns + TTL + 数据类型），
//! 产出人读的 `redis/key-design.md` 与机读的 `redis/key-design.json`，供部署 Redis 时
//! 直接对照（缓存预热 / 失效、会话存储、限流、任务锁、SSE 订阅集合）。
//!
//! 键名统一带边界前缀 `forma:{project}:{version}:...`，便于多项目实例隔离；
//! 不依赖真实 Redis 运行时（零新增依赖），只做纯函数派生 + 确定性输出。

use crate::dsl::Program;
use crate::generators::GeneratedFile;
use crate::identity::to_snake_case;
use std::path::Path;

/// `X | Y` = 是 / 否；空 = N/A
const DATA_CACHE: &str = "Hash/String(JSON)";

fn key_design(program: &Program) -> (String, Vec<KeySpec>) {
    let app_name = program
        .app
        .as_ref()
        .map(|a| a.name.clone())
        .unwrap_or_else(|| "app".to_string());
    let version = program
        .app
        .as_ref()
        .map(|a| a.version.clone())
        .unwrap_or_else(|| "0.0.0".to_string());
    let prefix = format!("forma:{}:{}", to_snake_case(&app_name), version);

    let mut keys: Vec<KeySpec> = Vec::new();

    for entity in &program.entities {
        let e = to_snake_case(&entity.name);
        // 单实体缓存（读多 / 写后失效）
        keys.push(KeySpec {
            kind: "cache",
            key: format!("{prefix}:cache:{e}:{{id}}"),
            purpose: format!("{} 单实体 JSON 缓存（写后失效）", e),
            datatype: DATA_CACHE.into(),
            ttl_secs: 300,
        });
        // 列表缓存（同过滤串复用；写后逐键失效）
        keys.push(KeySpec {
            kind: "cache",
            key: format!("{prefix}:cache:{e}:list"),
            purpose: format!("{} 列表查询缓存（按过滤串哈希）", e),
            datatype: DATA_CACHE.into(),
            ttl_secs: 60,
        });
        // 索引 → Sorted Set（score = 排序值）
        for idx in &entity.indexes {
            let cols = idx.join("_");
            keys.push(KeySpec {
                kind: "index",
                key: format!("{prefix}:idx:{e}:{cols}"),
                purpose: format!("{} 索引 ({}) 有序集", e, idx.join(", ")),
                datatype: "ZSET".into(),
                ttl_secs: 0,
            });
        }
        // 唯一约束 → SET（存在性 / 反查 id）
        for uq in &entity.uniques {
            let cols = uq.join("_");
            keys.push(KeySpec {
                kind: "unique",
                key: format!("{prefix}:uq:{e}:{cols}:{{value}}"),
                purpose: format!("{} 唯一约束 ({}) 反查 / 存在性", e, uq.join(", ")),
                datatype: "SET".into(),
                ttl_secs: 86400,
            });
        }
    }

    // 认证会话（auth module 存在时）
    if let Some(auth) = &program.auth {
        let ttl = auth.token_hours * 3600;
        keys.push(KeySpec {
            kind: "session",
            key: format!("{prefix}:session:{{token}}"),
            purpose: format!("登录会话 (token 有效期 {}h)", auth.token_hours),
            datatype: "STRING(JSON)".into(),
            ttl_secs: ttl,
        });
        keys.push(KeySpec {
            kind: "session",
            key: format!("{prefix}:session:user:{{user}}"),
            purpose: "user → 会话集合（多端登出）".to_string(),
            datatype: "SET".into(),
            ttl_secs: ttl,
        });
    }

    // 定时任务：上次触发时间（幂等/锁）
    for job in &program.jobs {
        keys.push(KeySpec {
            kind: "job",
            key: format!("{prefix}:job:{}:last_run", to_snake_case(&job.name)),
            purpose: format!("{} 上次执行时间戳", job.name),
            datatype: "STRING(int)".into(),
            ttl_secs: 0,
        });
    }

    // 实时推送：订阅集合
    for live in &program.lives {
        keys.push(KeySpec {
            kind: "live",
            key: format!("{prefix}:live:{}:subscribers", to_snake_case(&live.name)),
            purpose: format!("{} 通道订阅者集合", live.name),
            datatype: "SET".into(),
            ttl_secs: 0,
        });
    }

    // 限流（通用计数器）
    keys.push(KeySpec {
        kind: "ratelimit",
        key: format!("{prefix}:rl:{{key}}:{{window}}"),
        purpose: "令牌桶 / 计数器限流窗口".to_string(),
        datatype: "STRING(int)".into(),
        ttl_secs: 60,
    });

    (prefix, keys)
}

struct KeySpec {
    kind: &'static str,
    key: String,
    purpose: String,
    datatype: String,
    ttl_secs: u64,
}

fn render_md(prefix: &str, keys: &[KeySpec]) -> String {
    let mut out = String::new();
    out.push_str("# Redis 键设计（由 Forma 从 DSL 派生）\n\n");
    out.push_str("- 前缀：`");
    out.push_str(prefix);
    out.push_str("`\n");
    out.push_str("- 用途：缓存 / 会话 / 限流 / 任务锁 / SSE 订阅集合\n");
    out.push_str("- TTL(s) = 0 表示持久化\n\n");
    out.push_str("| Key | 用途 | 数据类型 | TTL(s) | 类型 |\n");
    out.push_str("|---|---|---|---|---|\n");
    for k in keys {
        out.push_str(&format!(
            "| `{}` | {} | {} | {} | {} |\n",
            k.key,
            k.purpose,
            k.datatype,
            if k.ttl_secs == 0 {
                "∞".to_string()
            } else {
                k.ttl_secs.to_string()
            },
            k.kind
        ));
    }
    out
}

fn render_json(prefix: &str, keys: &[KeySpec]) -> String {
    let list: Vec<serde_json::Value> = keys.iter().map(|k| {
        serde_json::json!({
            "kind": k.kind,
            "key": k.key,
            "purpose": k.purpose,
            "datatype": k.datatype,
            "ttlSecs": k.ttl_secs,
        })
    })
    .collect();
    serde_json::to_string_pretty(&serde_json::json!({
        "prefix": prefix,
        "generatedBy": "gen-redis",
        "description": "Redis key design derived from DSL (cache/session/ratelimit/job/live)",
        "keyCount": keys.len(),
        "keys": list,
    }))
    .unwrap()
}

pub fn emit_redis(
    project_dir: &Path,
    program: &Program,
    out_dir: &str,
) -> anyhow::Result<Vec<GeneratedFile>> {
    if program.entities.is_empty() {
        return Ok(vec![]);
    }
    let (prefix, keys) = key_design(program);
    let md = render_md(&prefix, &keys);
    let json = render_json(&prefix, &keys);
    let files = vec![
        GeneratedFile {
            path: format!("{out_dir}/redis/key-design.md"),
            content: md,
            generator: "gen-redis".into(),
        },
        GeneratedFile {
            path: format!("{out_dir}/redis/key-design.json"),
            content: json,
            generator: "gen-redis".into(),
        },
    ];
    for f in &files {
        crate::util::write_text(&project_dir.join(&f.path), &f.content)?;
    }
    Ok(files)
}
