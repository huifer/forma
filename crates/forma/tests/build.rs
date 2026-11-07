use forma::engine::{build_project, BuildOptions};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// 进程内递增序号：SystemTime 的 Debug 格式纳秒会被截断，同二进制内两个
/// 测试线程在同一窗口会得到同名目录。序号保证同进程内唯一。
static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// 联邦工程 fixture：`forma init` 现在生成 v0.3 DSL 工程；
/// 联邦管线（engine.build_project）的测试用仓库里的 order-management 原型。
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
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/order-management")
        .canonicalize()
        .unwrap();
    copy_dir(&src, &dir);
    dir
}

fn copy_dir(src: &PathBuf, dst: &PathBuf) {
    fs::create_dir_all(dst).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().to_string();
        // 只拷源文件：跳过本机构建缓存（.forma 增量缓存会让 fixture 首建即“无产物”）
        if matches!(
            name.as_str(),
            ".forma" | "generated" | "node_modules" | "target" | "dist"
        ) {
            continue;
        }
        let target = dst.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

#[test]
fn build_generates_all_four_targets_deterministically() {
    let dir = tmp_project();
    let first = build_project(&BuildOptions {
        project_dir: dir.clone(),
        force: false,
        entities: None,
        dry_run: false,
        ownership: forma::build::OwnershipPolicy::default(),
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
    assert!(paths.contains(&"generated/frontend/OrderForm.tsx"));

    // 确定性：第二次 force 构建产物一致
    let second = build_project(&BuildOptions {
        project_dir: dir.clone(),
        force: true,
        entities: None,
        dry_run: false,
        ownership: forma::build::OwnershipPolicy::default(),
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
        ownership: forma::build::OwnershipPolicy::default(),
    })
    .unwrap();
    let second = build_project(&BuildOptions {
        project_dir: dir.clone(),
        force: false,
        entities: None,
        dry_run: false,
        ownership: forma::build::OwnershipPolicy::default(),
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
        ownership: forma::build::OwnershipPolicy::default(),
    })
    .unwrap();
    let types = outcome
        .files
        .iter()
        .find(|f| f.path.ends_with("types/types.ts"))
        .unwrap();
    assert!(types.content.contains("export interface Order {"));
    assert!(types
        .content
        .contains("status: \"pending\" | \"approved\" | \"rejected\""));
    let validators = outcome
        .files
        .iter()
        .find(|f| f.path.ends_with("validators.ts"))
        .unwrap();
    assert!(validators.content.contains("export function isValidOrder"));
    assert!(validators
        .content
        .contains("[\"pending\", \"approved\", \"rejected\"].includes"));
    assert!(validators
        .content
        .contains("import type { Order } from './types.js';"));
}

#[test]
fn generated_hono_routes_contain_operations_with_validation() {
    let dir = tmp_project();
    let outcome = build_project(&BuildOptions {
        project_dir: dir.clone(),
        force: false,
        entities: None,
        dry_run: false,
        ownership: forma::build::OwnershipPolicy::default(),
    })
    .unwrap();
    let routes = outcome
        .files
        .iter()
        .find(|f| f.path.ends_with("backend/routes.ts"))
        .unwrap();
    assert!(routes.content.contains("orderRoutes.post('/orders'"));
    assert!(routes.content.contains("isValid"));
    assert!(routes.content.contains("orderRoutes.get('/orders/:id'"));
}

#[test]
fn generated_react_form_renders_controls_from_ui_schema() {
    let dir = tmp_project();
    let outcome = build_project(&BuildOptions {
        project_dir: dir.clone(),
        force: false,
        entities: None,
        dry_run: false,
        ownership: forma::build::OwnershipPolicy::default(),
    })
    .unwrap();
    let form = outcome
        .files
        .iter()
        .find(|f| f.path.ends_with("OrderForm.tsx"))
        .unwrap();
    assert!(form.content.contains("export function OrderForm"));
    assert!(form.content.contains("select")); // status enum → select
}
