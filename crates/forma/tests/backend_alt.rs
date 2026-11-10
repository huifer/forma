//! 后端扩栈（轨道 B 重新排期）回归：Go/Gin、PHP/Laravel、.NET 8 路由与产物、显式拒绝、默认零变化。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

fn tmp_dir(tag: &str) -> PathBuf {
    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "forma-balt-{tag}-{}-{seq}-{nanos}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("forma")).unwrap();
    dir
}

const FM: &str = r#"
app baltdemo { version = "0.1.0" }
entity Order {
  id Uuid @id @default(uuid)
  title String(80) @required
  amount Decimal(12,2)
  paid Bool
}
api Orders { entity Order operations [list get create update delete] }
"#;

fn write_project(dir: &Path, language: &str, framework: &str) {
    fs::write(
        dir.join("forma.yaml"),
        format!(
            "project:\n  name: baltdemo\n  version: 0.1.0\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n  backend:\n    language: {language}\n    framework: {framework}\n    out: generated/backend\n"
        ),
    )
    .unwrap();
    fs::write(dir.join("forma/app.fm"), FM).unwrap();
}

#[test]
fn go_gin_target_generates_gorm_models_and_routes() {
    let dir = tmp_dir("gin");
    write_project(&dir, "go", "gin");
    let report = forma::build::run_build(&dir).unwrap();
    let names: Vec<&str> = report.steps.iter().map(|step| step.name).collect();
    assert!(names.contains(&"backend-gin"), "{names:?}");
    let gomod = fs::read_to_string(dir.join("generated/backend/go.mod")).unwrap();
    assert!(gomod.contains("gin-gonic/gin"), "{gomod}");
    let main = fs::read_to_string(dir.join("generated/backend/main.go")).unwrap();
    assert!(main.contains("r.GET(\"/api/orders\""), "{main}");
    assert!(main.contains("r.POST(\"/api/orders\""), "{main}");
    let models = fs::read_to_string(dir.join("generated/backend/models.go")).unwrap();
    assert!(models.contains("type Order struct"), "{models}");
    assert!(models.contains("float64"), "{models}");
    assert!(!models.contains("import \"time\""), "{models}"); // 无时间字段 → 不导入 time
    assert!(gomod.trim_end().ends_with(')'), "{gomod}");
}

#[test]
fn go_gin_imports_time_when_temporal_fields_exist() {
    let dir = tmp_dir("gin-time");
    write_project(&dir, "go", "gin");
    fs::write(
        dir.join("forma/app.fm"),
        r#"
app baltdemo { version = "0.1.0" }
entity Event {
  id Uuid @id @default(uuid)
  at Timestamp
}
api Events { entity Event operations [list get create update delete] }
"#,
    )
    .unwrap();
    forma::build::run_build(&dir).unwrap();
    let models = fs::read_to_string(dir.join("generated/backend/models.go")).unwrap();
    assert!(models.contains("import \"time\""), "{models}");
    assert!(models.contains("time.Time"), "{models}");
}

#[test]
fn php_laravel_target_generates_eloquent_models_and_controllers() {
    let dir = tmp_dir("laravel");
    write_project(&dir, "php", "laravel");
    let report = forma::build::run_build(&dir).unwrap();
    let names: Vec<&str> = report.steps.iter().map(|step| step.name).collect();
    assert!(names.contains(&"backend-laravel"), "{names:?}");
    let model = fs::read_to_string(dir.join("generated/backend/app/Models/Order.php")).unwrap();
    assert!(model.contains("extends Model"), "{model}");
    assert!(model.contains("protected $table = 'orders'"), "{model}");
    let ctrl =
        fs::read_to_string(dir.join("generated/backend/app/Http/Controllers/OrderController.php"))
            .unwrap();
    assert!(ctrl.contains("function index"), "{ctrl}");
    assert!(ctrl.contains("function store"), "{ctrl}");
    let composer = fs::read_to_string(dir.join("generated/backend/composer.json")).unwrap();
    assert!(composer.contains("laravel/framework"), "{composer}");
}

#[test]
fn dotnet_target_generates_minimal_api_project() {
    let dir = tmp_dir("dotnet");
    write_project(&dir, "csharp", "minimal");
    let report = forma::build::run_build(&dir).unwrap();
    let names: Vec<&str> = report.steps.iter().map(|step| step.name).collect();
    assert!(names.contains(&"backend-dotnet"), "{names:?}");
    let csproj = fs::read_to_string(dir.join("generated/backend/baltdemo.csproj")).unwrap();
    assert!(csproj.contains("net8.0"), "{csproj}");
    let program = fs::read_to_string(dir.join("generated/backend/Program.cs")).unwrap();
    assert!(program.contains("MapGet(\"/api/orders\""), "{program}");
    let models = fs::read_to_string(dir.join("generated/backend/Models.cs")).unwrap();
    assert!(models.contains("public class Order"), "{models}");
}

#[test]
fn rust_default_and_unsupported_semantics() {
    let dir = tmp_dir("default");
    write_project(&dir, "rust", "axum");
    let report = forma::build::run_build(&dir).unwrap();
    let names: Vec<&str> = report.steps.iter().map(|step| step.name).collect();
    assert!(names.contains(&"backend-rust"), "{names:?}");
    assert!(!names.contains(&"backend-gin"), "{names:?}");

    let bad = tmp_dir("reject");
    write_project(&bad, "go", "gin");
    fs::write(
        bad.join("forma/app.fm"),
        r#"
app baltdemo { version = "0.1.0" }
entity Order { id Uuid @id @default(uuid) }
api Orders { entity Order operations [list get create update delete] }
auth module { provider jwt roles [admin] }
"#,
    )
    .unwrap();
    let err = forma::build::run_build(&bad).unwrap_err();
    assert!(err.to_string().contains("does not support"), "{err}");
}
