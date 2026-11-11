//! 部署产物（M11 / P1）回归测试：`deployment:` manifest 段 opt-in。
//!
//! 验收：
//! - kubernetes：6 份 manifests，serde_yaml 可解析，replicas/image/host/namespace/探活正确；
//! - helm：Chart.yaml / values.yaml 可解析 + templates 合法 Go 模板；
//! - terraform：四份 .tf + 关键资源块 + 括号平衡；
//! - all：三套全生成；
//! - 无 deployment 段：零产物（byte 级零变化）；
//! - 未知 provider / 非 Rust backend：构建期显式报错（宪法 6.2）。

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
        "forma-deploy-{tag}-{}-{seq}-{nanos}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

const APP: &str = r#"app deploydemo {
  version = "0.1.0"
}

entity Item {
  id   Uuid @id @default(uuid)
  name String(50) @required
}

api Items {
  entity Item
  operations [list get create update delete]
}
"#;

fn write_project(dir: &Path, deployment: Option<&str>, backend_rust: bool) {
    fs::create_dir_all(dir.join("forma")).unwrap();
    let backend = if backend_rust {
        "  backend:\n    language: rust\n    framework: axum\n    database: sqlx\n    out: generated/backend\n"
    } else {
        "  backend:\n    language: typescript\n    framework: hono\n    out: generated/backend\n"
    };
    let dep = deployment
        .map(|d| format!("deployment:\n{d}\n"))
        .unwrap_or_default();
    fs::write(
        dir.join("forma.yaml"),
        format!(
            "project:\n  name: deploydemo\n  version: 0.1.0\n\nsource: [\"forma/**/*.fm\"]\n\ntargets:\n{backend}\n{dep}"
        ),
    )
    .unwrap();
    fs::write(dir.join("forma/app.fm"), APP).unwrap();
}

fn yaml(path: &Path) -> serde_yaml::Value {
    let text = fs::read_to_string(path).unwrap();
    serde_yaml::from_str(&text).unwrap_or_else(|e| panic!("invalid YAML {}: {e}", path.display()))
}

#[test]
fn kubernetes_provider_generates_valid_manifests() {
    let dir = tmp_dir("k8s");
    write_project(
        &dir,
        Some("  provider: kubernetes\n  replicas: 3\n  image: registry.example.test/forma-backend:1.0.0\n  host: api.example.test\n  namespace: forma"),
        true,
    );
    let report = forma::build::run_build(&dir).unwrap();
    assert!(report.steps.iter().any(|s| s.name == "deployment"));

    for name in [
        "namespace.yaml",
        "configmap.yaml",
        "secret.yaml",
        "deployment.yaml",
        "service.yaml",
        "ingress.yaml",
    ] {
        assert!(
            dir.join(format!("k8s/{name}")).exists(),
            "missing k8s/{name}"
        );
        yaml(&dir.join(format!("k8s/{name}"))); // 真实 YAML 解析门
    }
    let dep = yaml(&dir.join("k8s/deployment.yaml"));
    assert_eq!(dep["spec"]["replicas"].as_u64(), Some(3));
    assert_eq!(
        dep["spec"]["template"]["spec"]["containers"][0]["image"].as_str(),
        Some("registry.example.test/forma-backend:1.0.0")
    );
    assert_eq!(dep["metadata"]["namespace"].as_str(), Some("forma"));
    // 探针用应用真实的 HTTP 端点（/ready /health），不再是 tcpSocket
    assert_eq!(
        dep["spec"]["template"]["spec"]["containers"][0]["readinessProbe"]["httpGet"]["path"]
            .as_str(),
        Some("/ready")
    );
    assert_eq!(
        dep["spec"]["template"]["spec"]["containers"][0]["readinessProbe"]["httpGet"]["port"]
            .as_u64(),
        Some(8080)
    );
    assert_eq!(
        dep["spec"]["template"]["spec"]["containers"][0]["livenessProbe"]["httpGet"]["path"]
            .as_str(),
        Some("/health")
    );
    // 最小权限：非 root、禁提权、丢弃全部 capabilities
    let sc = &dep["spec"]["template"]["spec"]["containers"][0]["securityContext"];
    assert_eq!(sc["runAsNonRoot"].as_bool(), Some(true));
    assert_eq!(sc["runAsUser"].as_u64(), Some(10001));
    assert_eq!(sc["allowPrivilegeEscalation"].as_bool(), Some(false));
    let svc = yaml(&dir.join("k8s/service.yaml"));
    assert_eq!(svc["spec"]["ports"][0]["port"].as_u64(), Some(8080));
    let ing = yaml(&dir.join("k8s/ingress.yaml"));
    assert_eq!(
        ing["spec"]["rules"][0]["host"].as_str(),
        Some("api.example.test")
    );
    let secret = fs::read_to_string(dir.join("k8s/secret.yaml")).unwrap();
    assert!(secret.contains("DATABASE_URL"));
    assert!(
        !secret.contains("JWT_SECRET:"),
        "no auth module → no JWT line"
    );
}

#[test]
fn helm_and_terraform_generation() {
    let dir = tmp_dir("helm-tf");
    write_project(
        &dir,
        Some("  provider: all\n  replicas: 2\n  namespace: forma"),
        true,
    );
    forma::build::run_build(&dir).unwrap();

    // Helm
    let chart = yaml(&dir.join("helm/deploydemo/Chart.yaml"));
    assert_eq!(chart["apiVersion"].as_str(), Some("v2"));
    assert_eq!(chart["appVersion"].as_str(), Some("0.1.0"));
    let values = yaml(&dir.join("helm/deploydemo/values.yaml"));
    assert_eq!(values["replicaCount"].as_u64(), Some(2));
    assert!(
        !fs::read_to_string(dir.join("helm/deploydemo/values.yaml"))
            .unwrap()
            .contains("{{"),
        "values.yaml must contain no Go template tokens"
    );
    let helm_dep =
        fs::read_to_string(dir.join("helm/deploydemo/templates/deployment.yaml")).unwrap();
    assert!(helm_dep.contains("{{ .Values.replicaCount }}"));
    assert!(helm_dep.contains("deploydemo.name"));

    // Terraform
    let main = fs::read_to_string(dir.join("terraform/main.tf")).unwrap();
    for block in [
        "resource \"kubernetes_namespace\" \"app\"",
        "resource \"kubernetes_deployment\" \"backend\"",
        "resource \"kubernetes_service\" \"backend\"",
        "readiness_probe",
    ] {
        assert!(main.contains(block), "main.tf missing {block}");
    }
    assert_eq!(
        main.matches('{').count(),
        main.matches('}').count(),
        "HCL braces unbalanced"
    );
    let vars = fs::read_to_string(dir.join("terraform/variables.tf")).unwrap();
    assert!(vars.contains("sensitive = true"));
    assert!(dir.join("terraform/versions.tf").exists());
    assert!(dir.join("terraform/outputs.tf").exists());
}

#[test]
fn no_deployment_section_has_zero_artifacts() {
    let dir = tmp_dir("zero");
    write_project(&dir, None, true);
    let report = forma::build::run_build(&dir).unwrap();
    assert!(!report.steps.iter().any(|s| s.name == "deployment"));
    for p in ["k8s", "helm", "terraform"] {
        assert!(!dir.join(p).exists(), "{p} must not exist");
    }
}

#[test]
fn unsupported_configuration_is_rejected_explicitly() {
    // 未知 provider
    let dir = tmp_dir("bad-provider");
    write_project(&dir, Some("  provider: nomad"), true);
    let err = forma::build::run_build(&dir).unwrap_err();
    assert!(
        err.to_string().contains("unsupported deployment provider"),
        "{err}"
    );

    // 非 Rust backend
    let dir = tmp_dir("no-rust");
    write_project(&dir, Some("  provider: kubernetes"), false);
    let err = forma::build::run_build(&dir).unwrap_err();
    assert!(err.to_string().contains("Rust/axum backend"), "{err}");
}
