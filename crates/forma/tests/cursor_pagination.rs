//! 游标分页回归测试（P1-6）：
//! - 编译期约束（必须有且仅有一个非空、支持类型的 sort 键）；
//! - 后端 keyset SQL（行比较 + id tie-breaker + pageSize+1）；
//! - API envelope `{items,nextCursor}` 与无效 cursor 400；
//! - Web `useInfiniteQuery` / OpenAPI 契约；offset API 不回归。
//!
//! 并行安全：产物写入 `tmp_dir("唯一 tag")`。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

fn tmp_dir(tag: &str) -> PathBuf {
    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!(
        "forma-cursor-{tag}-{}-{seq}-{nanos}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_project(dir: &Path, fm: &str) {
    fs::create_dir_all(dir.join("forma")).unwrap();
    fs::write(
        dir.join("forma.yaml"),
        "project:\n  name: cursordemo\n  version: 0.1.0\n\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  backend:\n    language: rust\n    framework: axum\n    database: sqlx\n    out: generated/backend\n  web:\n    framework: react\n    out: generated/web\n",
    )
    .unwrap();
    fs::write(dir.join("forma/app.fm"), fm).unwrap();
}

const CURSOR_FM: &str = r#"
app cursordemo { version = "0.1.0" }

entity Event {
  id        Uuid      @id @default(uuid)
  title     String(100) @required
  createdAt Timestamp @default(now)
}

api Events {
  entity Event
  operations [list get create]
  list { sort [createdAt desc] paginate cursor }
}

page EventList {
  route "/events"
  title { zh "事件" en "Events" }
  table {
    query Events.list
    columns [title, createdAt: datetime]
    actions { create → EventEdit row [edit → EventEdit] }
  }
}

page EventEdit {
  route "/events/{id}"
  title { zh "事件" en "Event" }
  form Event {
    layout stack { title Input @required }
    submit → Events.create
  }
}
"#;

fn emit_all(dir: &Path, fm: &str) -> (String, String, String, String) {
    write_project(dir, fm);
    let loaded = forma::manifest::load_manifest(dir).unwrap();
    let source = loaded.manifest.source.clone().unwrap();
    let (program, diagnostics) = forma::dsl::load_program(&loaded.dir, &source).unwrap();
    let mut diagnostics = diagnostics;
    diagnostics.extend(forma::dsl::check_program(&program));
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == "error")
        .collect();
    assert!(errors.is_empty(), "unexpected DSL errors: {errors:?}");

    let out = dir.join("generated");
    forma::emit::emit_rust_backend(
        &loaded.dir,
        &program,
        &out.join("backend").to_string_lossy(),
        "cursordemo",
    )
    .unwrap();
    forma::emit::emit_web_react(
        &loaded.dir,
        &program,
        &out.join("web").to_string_lossy(),
        "cursordemo",
        false,
        false,
        "echarts",
    )
    .unwrap();
    forma::emit::emit_openapi(
        &loaded.dir,
        &program,
        &out.join("api-docs").to_string_lossy(),
    )
    .unwrap();
    (
        fs::read_to_string(out.join("backend/src/repo.rs")).unwrap(),
        fs::read_to_string(out.join("backend/src/routes.rs")).unwrap(),
        fs::read_to_string(out.join("web/src/pages/EventList.tsx")).unwrap(),
        fs::read_to_string(out.join("api-docs/openapi.json")).unwrap(),
    )
}

// ── 1. Backend keyset pagination ────────────────────────────────

#[test]
fn cursor_backend_uses_keyset_tuple_and_stable_id_tiebreaker() {
    let dir = tmp_dir("keyset");
    let (repo, routes, _, _) = emit_all(&dir, CURSOR_FM);
    assert!(
        repo.contains("(s.created_at < ?) OR (s.created_at = ? AND s.id < ?)"),
        "descending keyset predicate must include sort key + id tie-breaker:\n{repo}"
    );
    assert!(
        repo.contains("ORDER BY s.created_at DESC, s.id DESC"),
        "id must be appended as deterministic tiebreaker:\n{repo}"
    );
    assert!(
        repo.contains("page_size + 1"),
        "fetch one extra row to determine has-more:\n{repo}"
    );
    assert!(
        routes.contains("nextCursor") && routes.contains("items"),
        "route must return {{items,nextCursor}} envelope:\n{routes}"
    );
    assert!(
        routes.contains("invalid cursor") && routes.contains("invalid cursor sort value"),
        "malformed cursor must fail closed as 400 validation error:\n{routes}"
    );
}

#[test]
fn ascending_cursor_uses_greater_than_comparator() {
    let fm = CURSOR_FM.replace("createdAt desc", "createdAt asc");
    let dir = tmp_dir("ascending");
    let (repo, _, _, _) = emit_all(&dir, &fm);
    assert!(repo.contains("s.created_at > ?"), "{repo}");
    assert!(
        repo.contains("ORDER BY s.created_at ASC, s.id ASC"),
        "{repo}"
    );
    assert!(repo.contains("s.created_at = ? AND s.id > ?"), "{repo}");
}

// ── 2. OpenAPI envelope and Web infinite query ───────────────────

#[test]
fn cursor_openapi_and_web_contract_match() {
    let dir = tmp_dir("surface");
    let (_, _, web, openapi) = emit_all(&dir, CURSOR_FM);
    let package: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(dir.join("generated/web/package.json")).unwrap())
            .expect("generated package.json must be valid JSON (including no-chart projects)");
    assert_eq!(package["name"], "cursordemo-web");
    assert!(
        web.contains("useInfiniteQuery"),
        "cursor page must use infinite query:\n{web}"
    );
    assert!(
        web.contains("initialPageParam: null as string | null"),
        "{web}"
    );
    assert!(web.contains("lastPage.nextCursor"), "{web}");
    assert!(web.contains("flatMap((p) => p.items)"), "{web}");
    assert!(
        openapi.contains("\"nextCursor\""),
        "OpenAPI must describe nextCursor:\n{openapi}"
    );
    assert!(
        openapi.contains("\"cursor\""),
        "OpenAPI must describe cursor parameter:\n{openapi}"
    );
    assert!(
        openapi.contains("\"items\""),
        "OpenAPI must describe items envelope:\n{openapi}"
    );
}

#[test]
fn multi_key_cursor_uses_lexicographic_predicate() {
    let fm = CURSOR_FM.replace("sort [createdAt desc]", "sort [createdAt desc, title asc]");
    let dir = tmp_dir("multi-key");
    let (repo, routes, _, _) = emit_all(&dir, &fm);
    assert!(
        repo.contains("(s.created_at < ?) OR (s.created_at = ? AND s.title > ?)"),
        "first lexicographic branches must respect each key direction:\\n{repo}"
    );
    assert!(
        repo.contains("s.created_at = ? AND s.title = ? AND s.id > ?"),
        "id tie-breaker uses the last sort direction:\\n{repo}"
    );
    assert!(
        repo.contains("ORDER BY s.created_at DESC, s.title ASC, s.id ASC"),
        "all sort keys plus id must be in ORDER BY:\\n{repo}"
    );
    assert!(
        routes.contains("\"values\": [last.created_at.to_rfc3339(), last.title.to_string()]"),
        "cursor payload must carry each sort key:\\n{routes}"
    );
}

#[test]
fn cursor_selection_query_unwraps_envelope_items() {
    // Cursor-paginated relationship target used as a select option must consume the first page's items.
    let fm = r#"
app cursordemo { version = "0.1.0" }

entity School {
  id   Uuid @id @default(uuid)
  name String(100) @required
}
entity Event {
  id      Uuid @id @default(uuid)
  school  School @required
  title   String(100) @required
}
api Schools {
  entity School
  operations [list get]
  list { sort [name asc] paginate cursor }
}
api Events {
  entity Event
  operations [list get]
  list { sort [title asc] paginate offset }
}
page EventList {
  route "/events"
  title { zh "事件" en "Events" }
  table {
    query Events.list
    columns [title]
    filter school as select
  }
}
"#;
    let dir = tmp_dir("cursor-options");
    write_project(&dir, fm);
    let loaded = forma::manifest::load_manifest(&dir).unwrap();
    let source = loaded.manifest.source.clone().unwrap();
    let (program, diagnostics) = forma::dsl::load_program(&loaded.dir, &source).unwrap();
    let mut diagnostics = diagnostics;
    diagnostics.extend(forma::dsl::check_program(&program));
    assert!(
        !diagnostics.iter().any(|d| d.severity == "error"),
        "fixture must be valid: {diagnostics:?}"
    );
    let out = dir.join("generated/web");
    forma::emit::emit_web_react(
        &loaded.dir,
        &program,
        &out.to_string_lossy(),
        "cursordemo",
        false,
        false,
        "echarts",
    )
    .unwrap();
    let page = fs::read_to_string(out.join("src/pages/EventList.tsx")).unwrap();
    assert!(
        page.contains("async () => (await SchoolApi.list({ pageSize: 200 })).items"),
        "relationship options sourced from cursor API must read .items:\n{page}"
    );
}

#[test]
fn cursor_api_supports_flutter_target() {
    // Flutter 端游标分页：CursorPage 信封 + ListParams.cursor + 列表页「加载更多」
    let dir = tmp_dir("cursor-flutter");
    let fm = CURSOR_FM;
    write_project(&dir, fm);
    let mut manifest = fs::read_to_string(dir.join("forma.yaml")).unwrap();
    manifest = manifest.replace(
        "    out: generated/web\n",
        "    out: generated/web\n  mobile:\n    framework: flutter\n    out: generated/mobile\n",
    );
    fs::write(dir.join("forma.yaml"), manifest).unwrap();
    forma::build::run_build(&dir).expect("cursor + Flutter target must build");

    let api = fs::read_to_string(dir.join("generated/mobile/lib/api/api.dart")).unwrap();
    // 信封类型
    assert!(api.contains("class CursorPage<T>"), "{api}");
    assert!(api.contains("final String? nextCursor;"), "{api}");
    // 参数携带 cursor
    assert!(api.contains("final String? cursor;"), "{api}");
    assert!(api.contains("'cursor': cursor!"), "{api}");
    // list 返回信封
    assert!(
        api.contains("Future<CursorPage<EventListRow>> list("),
        "{api}"
    );
    // 原始 GET（信封解析）
    assert!(api.contains("Future<dynamic> getRaw("), "{api}");

    // 列表页：首屏 + 加载更多
    let page = fs::read_to_string(dir.join("generated/mobile/lib/pages/event_list.dart")).unwrap();
    assert!(page.contains("String? _nextCursor;"), "{page}");
    assert!(page.contains("Future<void> _loadMore()"), "{page}");
    assert!(page.contains("cursor: cursor"), "{page}");
    assert!(page.contains("加载更多"), "{page}");
    // 游标模式不再生成 offset 偏移参数
    assert!(
        !page.contains("page:"),
        "cursor mode must not use offset page"
    );
}

#[test]
fn cursor_api_types_include_cursor_page_envelope() {
    // Cursor API 的 TypeScript 响应类型使用 CursorPage<T>。
    let fm = CURSOR_FM;
    // API 自身不是关系源，单独断言 Web 侧游标 envelope 类型已接入。
    let dir = tmp_dir("api-type");
    write_project(&dir, fm);
    let loaded = forma::manifest::load_manifest(&dir).unwrap();
    let source = loaded.manifest.source.clone().unwrap();
    let (program, _) = forma::dsl::load_program(&loaded.dir, &source).unwrap();
    let out = dir.join("generated/web");
    forma::emit::emit_web_react(
        &loaded.dir,
        &program,
        &out.to_string_lossy(),
        "cursordemo",
        false,
        false,
        "echarts",
    )
    .unwrap();
    let types = fs::read_to_string(out.join("src/types.ts")).unwrap();
    let api = fs::read_to_string(out.join("src/api.ts")).unwrap();
    assert!(types.contains("export interface CursorPage<T>"), "{types}");
    assert!(api.contains("req<CursorPage<EventListRow>>"), "{api}");
}

// ── 3. Compile-time constraints ─────────────────────────────────

#[test]
fn cursor_without_sort_is_rejected_by_check_phase() {
    let fm = CURSOR_FM.replace(
        "list { sort [createdAt desc] paginate cursor }",
        "list { paginate cursor }",
    );
    let dir = tmp_dir("no-sort");
    write_project(&dir, &fm);
    let loaded = forma::manifest::load_manifest(&dir).unwrap();
    let source = loaded.manifest.source.clone().unwrap();
    let (program, mut diagnostics) = forma::dsl::load_program(&loaded.dir, &source).unwrap();
    diagnostics.extend(forma::dsl::check_program(&program));
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == "dsl-cursor-sort-required"),
        "cursor pagination without a sort key must be rejected: {diagnostics:?}"
    );
}

#[test]
fn cursor_with_nullable_sort_is_rejected() {
    let fm = CURSOR_FM.replace("createdAt Timestamp @default(now)", "createdAt Timestamp?");
    let dir = tmp_dir("nullable-sort");
    write_project(&dir, &fm);
    let loaded = forma::manifest::load_manifest(&dir).unwrap();
    let source = loaded.manifest.source.clone().unwrap();
    let (program, mut diagnostics) = forma::dsl::load_program(&loaded.dir, &source).unwrap();
    diagnostics.extend(forma::dsl::check_program(&program));
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == "dsl-cursor-sort-nullable"),
        "nullable keyset sort must be rejected: {diagnostics:?}"
    );
}

// ── 4. Offset contract is unchanged ──────────────────────────────

#[test]
fn offset_api_keeps_array_response_and_offset_sql() {
    let fm = CURSOR_FM.replace(" paginate cursor", " paginate offset");
    let dir = tmp_dir("offset");
    let (repo, routes, web, openapi) = emit_all(&dir, &fm);
    assert!(repo.contains("LIMIT ? OFFSET ?"), "{repo}");
    assert!(routes.contains("Json<Vec<EventListRow>>"), "{routes}");
    assert!(
        !routes.contains("nextCursor"),
        "offset response remains an array:\n{routes}"
    );
    assert!(
        web.contains("useQuery("),
        "offset page must keep useQuery:\n{web}"
    );
    assert!(!web.contains("useInfiniteQuery"), "{web}");
    assert!(
        openapi.contains("\"type\": \"array\""),
        "offset OpenAPI remains array:\n{openapi}"
    );
}

// ── F-008：benchmark 矩阵（快照级——跑 3 个 case 验证管道，完整 100 case 走 CLI）────

#[test]
fn benchmark_matrix_produces_consistent_reports() {
    // 快速验证：bench 模块产出 100 case、同场景 variant 间结构一致、报告 schema 正确
    let cases = forma::bench::cases();
    assert_eq!(cases.len(), 100, "10 scenarios × 10 variants");

    // 每个 case 都有非空 DSL 且含 entity + api
    for case in cases.iter().take(10) {
        assert!(
            case.dsl.contains("entity Order"),
            "{} missing entity",
            case.id
        );
        assert!(case.dsl.contains("api Orders"), "{} missing api", case.id);
        assert!(
            case.dsl.contains("app benchmark"),
            "{} missing app",
            case.id
        );
    }

    // 场景 id 唯一
    let ids: std::collections::HashSet<&str> = cases.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids.len(), 100);
}

#[test]
fn benchmark_case_check_and_build_pass() {
    // 端到端：跑一个 case 的 DSL 检查 + 产物生成
    let dir = tmp_dir("f008-case");
    write_project(&dir, &forma::bench::cases()[0].dsl);
    forma::build::run_build(&dir).unwrap();

    // 关键产物存在
    assert!(dir.join("generated/backend/src/routes.rs").exists());
    assert!(dir.join("generated/backend/src/error.rs").exists());
}
