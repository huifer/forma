use forma::engine::{build_project, BuildOptions};
use forma::scaffold::scaffold_project;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// 进程内递增序号：SystemTime 的 Debug 格式纳秒会被截断，同二进制内两个
/// 测试线程在同一窗口会得到同名目录（scaffold 报 "forma.yaml already exists"）。
/// 序号保证同进程内唯一，pid + 纳秒保证跨进程唯一。
static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

fn tmp_project() -> PathBuf {
    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir =
        std::env::temp_dir().join(format!("forma-build-{}-{seq}-{nanos}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    scaffold_project(&dir).unwrap();
    dir
}

#[test]
fn build_generates_all_four_targets_deterministically() {
    let dir = tmp_project();
    let first = build_project(&BuildOptions {
        project_dir: dir.clone(),
        force: false,
        entities: None,
        dry_run: false,
    })
    .unwrap();
    assert_eq!(
        first
            .diagnostics
            .iter()
            .filter(|d| d.severity == "error")
            .count(),
        0,
        "{:?}",
        first.diagnostics
    );

    let mut paths: Vec<&str> = first.files.iter().map(|f| f.path.as_str()).collect();
    paths.sort();
    assert!(paths.contains(&"generated/types/types.ts"), "{paths:?}");
    assert!(paths.contains(&"generated/types/validators.ts"));
    assert!(paths.contains(&"generated/backend/routes.ts"));
    assert!(paths.contains(&"generated/database/migrations/0001_init.sql"));
    assert!(paths.contains(&"generated/frontend/UserForm.tsx"));

    // 确定性：第二次 force 构建产物一致
    let second = build_project(&BuildOptions {
        project_dir: dir.clone(),
        force: true,
        entities: None,
        dry_run: false,
    })
    .unwrap();
    for f in &second.files {
        let prev = first.files.iter().find(|x| x.path == f.path).unwrap();
        assert_eq!(
            prev.content, f.content,
            "nondeterministic output: {}",
            f.path
        );
    }
}

#[test]
fn incremental_build_skips_unchanged_outputs() {
    let dir = tmp_project();
    build_project(&BuildOptions {
        project_dir: dir.clone(),
        force: false,
        entities: None,
        dry_run: false,
    })
    .unwrap();
    let second = build_project(&BuildOptions {
        project_dir: dir.clone(),
        force: false,
        entities: None,
        dry_run: false,
    })
    .unwrap();
    assert!(
        second.files.is_empty(),
        "nothing to regenerate: {:?}",
        second.files
    );
    assert!(second.skipped > 0);
}

#[test]
fn generated_types_shape() {
    let dir = tmp_project();
    let outcome = build_project(&BuildOptions {
        project_dir: dir.clone(),
        force: false,
        entities: None,
        dry_run: false,
    })
    .unwrap();
    let types = outcome
        .files
        .iter()
        .find(|f| f.path.ends_with("types/types.ts"))
        .unwrap();
    assert!(types.content.contains("export interface User {"));
    assert!(types
        .content
        .contains("status: \"active\" | \"suspended\" | \"closed\""));
    let validators = outcome
        .files
        .iter()
        .find(|f| f.path.ends_with("validators.ts"))
        .unwrap();
    assert!(validators.content.contains("export function isValidUser"));
    assert!(validators
        .content
        .contains("[\"active\", \"suspended\", \"closed\"].includes"));
    assert!(validators
        .content
        .contains("import type { User } from './types.js';"));
}

#[test]
fn generated_hono_routes_contain_operations_with_validation() {
    let dir = tmp_project();
    let outcome = build_project(&BuildOptions {
        project_dir: dir.clone(),
        force: false,
        entities: None,
        dry_run: false,
    })
    .unwrap();
    let routes = outcome
        .files
        .iter()
        .find(|f| f.path.ends_with("backend/routes.ts"))
        .unwrap();
    assert!(routes.content.contains("userRoutes.post('/users'"));
    assert!(routes.content.contains("isValidUser(body)"));
    assert!(routes.content.contains("userRoutes.get('/users/:id'"));
}

#[test]
fn generated_react_form_renders_controls_from_ui_schema() {
    let dir = tmp_project();
    let outcome = build_project(&BuildOptions {
        project_dir: dir.clone(),
        force: false,
        entities: None,
        dry_run: false,
    })
    .unwrap();
    let form = outcome
        .files
        .iter()
        .find(|f| f.path.ends_with("UserForm.tsx"))
        .unwrap();
    assert!(form.content.contains("export function UserForm"));
    assert!(form.content.contains("<span>Email"));
    assert!(form.content.contains("select")); // status enum → select
}
