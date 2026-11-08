//! Capability Registry（RFC-0004）：Manifest 模式、注册、依赖闭包与兼容性校验。
//!
//! 设计目标：**新增能力不必修改 Semantic Core**。能力由 Manifest 描述，
//! 注册表负责「发现 → 注册 → 依赖闭包 → 版本兼容 → 权限汇总」，
//! 编译器只消费注册表的结论，不认识任何具体能力。
//!
//! Manifest 字段定义见 `docs/architecture/specifications/manifests/capability-manifest.zh-CN.md`。

use crate::validate::Diagnostic;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Manifest 的 `apiVersion`。
pub const MANIFEST_API_VERSION: &str = "forma.dev/v1";
/// Forma IR（Tier 1 语义契约）版本。
pub const IR_VERSION: &str = "1.0.0";
/// 运行时版本。
pub const RUNTIME_VERSION: &str = "1.0.0";
/// 编译器版本。
pub const COMPILER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// 项目在 `forma.yaml` 中声明的能力选择。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CapabilitySelection {
    /// 显式启用的能力 id（依赖闭包由注册表补全）。
    pub enabled: Vec<String>,
    /// 版本 pin：能力 id → 版本约束（如 `>=1.0.0 <2.0.0`）。
    pub pins: Vec<(String, String)>,
}

// ── Manifest ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityManifest {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
    pub metadata: CapabilityMetadata,
    pub spec: CapabilitySpec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityMetadata {
    pub id: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CapabilitySpec {
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub provides: Vec<String>,
    #[serde(default)]
    pub consumes: Vec<String>,
    #[serde(default)]
    pub adapters: Vec<String>,
    #[serde(default)]
    pub runtime: Vec<String>,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub compatibility: Compatibility,
    #[serde(default)]
    pub conformance: Option<Conformance>,
    #[serde(default)]
    pub deprecated: Option<Deprecation>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Compatibility {
    #[serde(default)]
    pub ir: Option<String>,
    #[serde(default)]
    pub runtime: Option<String>,
}

/// RFC-0004 §8.4 / spec:compatibility §9：弃用契约的结构化字段。
///
/// 语义（随 resolve 构建期强制）：声明 `deprecated` 时 **MUST** 提供 `successor`——
/// 否则产出 `capability-deprecation-missing-successor` 错误并阻断构建；
/// 指向的 successor 必须已注册（`capability-deprecation-successor-unknown`）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Deprecation {
    /// 弃用原因（人读）
    #[serde(default)]
    pub reason: String,
    /// 继任能力 id（必填，spec:compatibility §9）
    #[serde(default)]
    pub successor: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Conformance {
    #[serde(default)]
    pub suite: Option<String>,
}

// ── 版本与范围 ──────────────────────────────────────────────────

/// 极简 semver（`major.minor.patch`，允许省略 patch）。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl Version {
    pub fn new(major: u64, minor: u64, patch: u64) -> Self {
        Version {
            major,
            minor,
            patch,
        }
    }

    pub fn parse(s: &str) -> Option<Version> {
        let s = s.trim();
        let core = s.split(['-', '+']).next()?;
        let mut parts = core.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next().unwrap_or("0").parse().ok()?;
        let patch = parts.next().unwrap_or("0").parse().ok()?;
        if parts.next().is_some() {
            return None;
        }
        Some(Version::new(major, minor, patch))
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// 判断版本 `v` 是否落在范围 `range` 内。
///
/// 支持 `>=` `>` `<=` `<` `=`（缺省）`^` `~` 与空格分隔的组合；空范围 / `*` 匹配任意版本。
pub fn version_matches(range: &str, v: &Version) -> bool {
    let range = range.trim();
    if range.is_empty() || range == "*" {
        return true;
    }
    range.split_whitespace().all(|c| comparator_matches(c, v))
}

fn comparator_matches(c: &str, v: &Version) -> bool {
    let (op, rest) = if let Some(r) = c.strip_prefix(">=") {
        (">=", r)
    } else if let Some(r) = c.strip_prefix("<=") {
        ("<=", r)
    } else if let Some(r) = c.strip_prefix('>') {
        (">", r)
    } else if let Some(r) = c.strip_prefix('<') {
        ("<", r)
    } else if let Some(r) = c.strip_prefix('^') {
        ("^", r)
    } else if let Some(r) = c.strip_prefix('~') {
        ("~", r)
    } else if let Some(r) = c.strip_prefix('=') {
        ("=", r)
    } else {
        ("=", c)
    };
    let Some(target) = Version::parse(rest) else {
        return false;
    };
    match op {
        ">=" => v >= &target,
        "<=" => v <= &target,
        ">" => v > &target,
        "<" => v < &target,
        "=" => v == &target,
        // caret：>=target，且 < 下一个不兼容版本（major>0 时为下一 major；major==0 时为下一 minor）
        "^" => {
            if v < &target {
                return false;
            }
            let upper = if target.major > 0 {
                Version::new(target.major + 1, 0, 0)
            } else {
                Version::new(0, target.minor + 1, 0)
            };
            v < &upper
        }
        // tilde：>=target，且 < 下一 minor
        "~" => {
            if v < &target {
                return false;
            }
            v < &Version::new(target.major, target.minor + 1, 0)
        }
        _ => false,
    }
}

// ── 注册表 ──────────────────────────────────────────────────────

/// 一个已解析的能力（依赖闭包中的成员）。
#[derive(Debug, Clone, Serialize)]
pub struct ResolvedCapability {
    pub id: String,
    pub version: String,
    pub category: String,
    /// 是否为项目直接声明（false = 依赖闭包带入）。
    pub explicit: bool,
    pub permissions: Vec<String>,
    pub adapters: Vec<String>,
}

/// 依赖解析结果。
#[derive(Debug, Clone, Default, Serialize)]
pub struct Resolution {
    /// 依赖闭包（拓扑序：被依赖者在前）。
    pub active: Vec<ResolvedCapability>,
    /// 全部权限声明的并集。
    pub permissions: Vec<String>,
    pub diagnostics: Vec<Diagnostic>,
}

impl Resolution {
    pub fn errors(&self) -> Vec<&Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == "error")
            .collect()
    }
}

/// 能力注册表：内置 Manifest + 项目自备 Manifest。
#[derive(Debug, Clone, Default)]
pub struct CapabilityRegistry {
    manifests: BTreeMap<String, CapabilityManifest>,
    sources: BTreeMap<String, String>,
    /// 编译器内置已实现的 adapter id。
    builtin_adapters: BTreeSet<String>,
}

impl CapabilityRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// 内置注册表：Forma 自带的能力（数据 / 可视化 / UI / 集成）。
    pub fn with_builtins() -> Self {
        let mut r = Self::new();
        r.builtin_adapters = builtin_adapters().into_iter().map(String::from).collect();
        for m in builtin_manifests() {
            let id = m.metadata.id.clone();
            r.sources.insert(id, "builtin".to_string());
            r.manifests.insert(m.metadata.id.clone(), m);
        }
        r
    }

    /// 注册一个 Manifest。返回错误原因（不 panic）。
    pub fn register(&mut self, m: CapabilityManifest, source: String) -> Result<(), String> {
        if m.kind != "Capability" {
            return Err(format!("kind must be `Capability`, got `{}`", m.kind));
        }
        if m.api_version != MANIFEST_API_VERSION {
            return Err(format!(
                "unsupported apiVersion `{}` (expected `{}`)",
                m.api_version, MANIFEST_API_VERSION
            ));
        }
        let id = m.metadata.id.clone();
        if !id.contains('.') || id.starts_with('.') || id.ends_with('.') {
            return Err(format!("capability id `{id}` must be `<category>.<name>`"));
        }
        if Version::parse(&m.metadata.version).is_none() {
            return Err(format!(
                "capability `{}` version `{}` is not semver",
                id, m.metadata.version
            ));
        }
        if self.manifests.contains_key(&id) {
            return Err(format!("duplicate capability id `{id}`"));
        }
        self.sources.insert(id.clone(), source);
        self.manifests.insert(id, m);
        Ok(())
    }

    /// 载入一段 Manifest 文本（`source` 用于诊断定位）。
    pub fn load_manifest_text(&mut self, text: &str, source: &str) -> Result<(), Vec<Diagnostic>> {
        let manifest: CapabilityManifest = match serde_yaml::from_str(text) {
            Ok(m) => m,
            Err(e) => {
                return Err(vec![Diagnostic {
                    severity: "error".into(),
                    code: "capability-manifest-invalid".into(),
                    message: format!("{source}: invalid manifest — {e}"),
                    file: Some(source.to_string()),
                    line: None,
                    nodes: None,
                }]);
            }
        };
        self.register(manifest, source.to_string()).map_err(|e| {
            vec![Diagnostic {
                severity: "error".into(),
                code: "capability-manifest-invalid".into(),
                message: format!("{source}: {e}"),
                file: Some(source.to_string()),
                line: None,
                nodes: None,
            }]
        })
    }

    /// 载入项目自备能力清单（`<project>/capabilities/*.yaml`）。
    ///
    /// 这就是「新增能力不必改核心」的落点：放一个 YAML 即可，编译器不需要重新编译。
    pub fn load_project_dir(&mut self, dir: &Path) -> Vec<Diagnostic> {
        let mut out = Vec::new();
        let cap_dir = dir.join("capabilities");
        let Ok(entries) = std::fs::read_dir(&cap_dir) else {
            return out;
        };
        let mut files: Vec<_> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.extension()
                    .map(|x| x.eq_ignore_ascii_case("yaml") || x.eq_ignore_ascii_case("yml"))
                    .unwrap_or(false)
            })
            .collect();
        files.sort();
        for f in files {
            let rel = f
                .strip_prefix(dir)
                .unwrap_or(&f)
                .to_string_lossy()
                .replace('\\', "/");
            let text = match std::fs::read_to_string(&f) {
                Ok(t) => t,
                Err(e) => {
                    out.push(Diagnostic {
                        severity: "error".into(),
                        code: "capability-read-error".into(),
                        message: format!("cannot read {rel}: {e}"),
                        file: Some(rel),
                        line: None,
                        nodes: None,
                    });
                    continue;
                }
            };
            if let Err(errs) = self.load_manifest_text(&text, &rel) {
                out.extend(errs);
            }
        }
        out
    }

    pub fn get(&self, id: &str) -> Option<&CapabilityManifest> {
        self.manifests.get(id)
    }

    pub fn source_of(&self, id: &str) -> Option<&String> {
        self.sources.get(id)
    }

    pub fn ids(&self) -> Vec<&String> {
        self.manifests.keys().collect()
    }

    pub fn manifests(&self) -> impl Iterator<Item = &CapabilityManifest> {
        self.manifests.values()
    }

    /// 解析启用的能力集合：依赖闭包 + 版本兼容 + 权限汇总。
    pub fn resolve(&self, enabled: &[String], pins: &[(String, String)]) -> Resolution {
        let mut res = Resolution::default();
        let mut explicit: BTreeSet<String> = BTreeSet::new();
        let mut order: Vec<String> = Vec::new();
        let mut visiting: Vec<String> = Vec::new();
        let mut seen: BTreeSet<String> = BTreeSet::new();

        for id in enabled {
            explicit.insert(id.clone());
            self.visit(
                id,
                &explicit,
                &mut seen,
                &mut order,
                &mut visiting,
                &mut res.diagnostics,
            );
        }

        // 版本 pin
        for (id, pin) in pins {
            match self.get(id) {
                None => res.diagnostics.push(Diagnostic {
                    severity: "error".into(),
                    code: "capability-unknown".into(),
                    message: format!("pinned capability `{id}` is not registered"),
                    file: None,
                    line: None,
                    nodes: None,
                }),
                Some(m) => {
                    let v = Version::parse(&m.metadata.version).unwrap_or(Version::new(0, 0, 0));
                    if !version_matches(pin, &v) {
                        res.diagnostics.push(Diagnostic {
                            severity: "error".into(),
                            code: "capability-pin-mismatch".into(),
                            message: format!(
                                "capability `{id}` version {} does not satisfy pin `{pin}`",
                                m.metadata.version
                            ),
                            file: None,
                            line: None,
                            nodes: None,
                        });
                    }
                }
            }
        }

        let mut permissions: BTreeSet<String> = BTreeSet::new();
        for id in &order {
            let Some(m) = self.get(id) else { continue };
            permissions.extend(m.spec.permissions.iter().cloned());
            for adapter in &m.spec.adapters {
                if !self.builtin_adapters.contains(adapter) {
                    res.diagnostics.push(Diagnostic {
                        severity: "warning".into(),
                        code: "capability-adapter-not-builtin".into(),
                        message: format!(
                            "capability `{id}` declares adapter `{adapter}`, which is not implemented in forma {COMPILER_VERSION}（Manifest 被接受，但不产生额外产物）"
                        ),
                        file: self.source_of(id).cloned(),
                        line: None,
                        nodes: None,
                    });
                }
            }
        }
        res.permissions = permissions.into_iter().collect();

        // RFC-0004 §8.4 / spec:compatibility §9：弃用契约（G-2 收口）
        for m in self.manifests() {
            let Some(dep) = &m.spec.deprecated else {
                continue;
            };
            let Some(successor) = &dep.successor else {
                res.diagnostics.push(Diagnostic {
                    severity: "error".into(),
                    code: "capability-deprecation-missing-successor".into(),
                    message: format!(
                        "capability `{}` is deprecated but declares no `successor` (spec:compatibility §9)",
                        m.metadata.id
                    ),
                    file: None,
                    line: None,
                    nodes: None,
                });
                continue;
            };
            if self.get(successor).is_none() {
                res.diagnostics.push(Diagnostic {
                    severity: "error".into(),
                    code: "capability-deprecation-successor-unknown".into(),
                    message: format!(
                        "capability `{}` deprecates to unknown successor `{successor}`",
                        m.metadata.id
                    ),
                    file: None,
                    line: None,
                    nodes: None,
                });
            }
        }

        for id in &order {
            let m = self.get(id).expect("resolved capability exists");
            res.active.push(ResolvedCapability {
                id: m.metadata.id.clone(),
                version: m.metadata.version.clone(),
                category: m.spec.category.clone(),
                explicit: explicit.contains(id),
                permissions: m.spec.permissions.clone(),
                adapters: m.spec.adapters.clone(),
            });
        }
        res
    }

    /// 深度优先解析依赖；`order` 为拓扑序（被依赖者在前）。
    fn visit(
        &self,
        id: &str,
        explicit: &BTreeSet<String>,
        seen: &mut BTreeSet<String>,
        order: &mut Vec<String>,
        visiting: &mut Vec<String>,
        diags: &mut Vec<Diagnostic>,
    ) {
        if seen.contains(id) {
            return;
        }
        if visiting.iter().any(|v| v == id) {
            let mut cycle = visiting.clone();
            cycle.push(id.to_string());
            diags.push(Diagnostic {
                severity: "error".into(),
                code: "capability-dep-cycle".into(),
                message: format!("capability dependency cycle: {}", cycle.join(" → ")),
                file: None,
                line: None,
                nodes: Some(cycle),
            });
            return;
        }
        let Some(m) = self.get(id) else {
            diags.push(Diagnostic {
                severity: "error".into(),
                code: "capability-unknown".into(),
                message: format!(
                    "capability `{id}` is not registered（内置或项目 capabilities/ 清单中都没有）"
                ),
                file: None,
                line: None,
                nodes: None,
            });
            return;
        };
        visiting.push(id.to_string());
        for dep in &m.spec.dependencies {
            self.visit(dep, explicit, seen, order, visiting, diags);
        }
        visiting.pop();
        if seen.insert(id.to_string()) {
            order.push(id.to_string());
            // 兼容性：IR / Runtime 版本范围
            let ir = Version::parse(IR_VERSION).expect("IR_VERSION is semver");
            if let Some(range) = &m.spec.compatibility.ir {
                if !version_matches(range, &ir) {
                    diags.push(Diagnostic {
                        severity: "error".into(),
                        code: "capability-ir-incompatible".into(),
                        message: format!(
                            "capability `{id}` requires IR `{range}`, but this compiler provides IR {IR_VERSION}"
                        ),
                        file: self.source_of(id).cloned(),
                        line: None,
                        nodes: None,
                    });
                }
            }
            let rt = Version::parse(RUNTIME_VERSION).expect("RUNTIME_VERSION is semver");
            if let Some(range) = &m.spec.compatibility.runtime {
                if !version_matches(range, &rt) {
                    diags.push(Diagnostic {
                        severity: "error".into(),
                        code: "capability-runtime-incompatible".into(),
                        message: format!(
                            "capability `{id}` requires runtime `{range}`, but this compiler provides runtime {RUNTIME_VERSION}"
                        ),
                        file: self.source_of(id).cloned(),
                        line: None,
                        nodes: None,
                    });
                }
            }
        }
        let _ = explicit;
    }
}

// ── 内置能力 ────────────────────────────────────────────────────

fn builtin_adapters() -> Vec<&'static str> {
    vec![
        "mysql-adapter",
        "echarts-adapter",
        "web-react-adapter",
        "flutter-adapter",
        "rust-backend-adapter",
        "openapi-adapter",
        "jsonschema-adapter",
        "sql-adapter",
        "binding-adapter",
    ]
}

/// 内置能力的声明（构造 Manifest 的输入）。
struct BuiltinSpec {
    id: &'static str,
    version: &'static str,
    description: &'static str,
    category: &'static str,
    provides: &'static [&'static str],
    consumes: &'static [&'static str],
    adapters: &'static [&'static str],
    runtime: &'static [&'static str],
    dependencies: &'static [&'static str],
    permissions: &'static [&'static str],
    suite: &'static str,
}

fn manifest(s: BuiltinSpec) -> CapabilityManifest {
    CapabilityManifest {
        api_version: MANIFEST_API_VERSION.to_string(),
        kind: "Capability".to_string(),
        metadata: CapabilityMetadata {
            id: s.id.to_string(),
            version: s.version.to_string(),
            description: Some(s.description.to_string()),
        },
        spec: CapabilitySpec {
            category: s.category.to_string(),
            provides: s.provides.iter().map(|x| x.to_string()).collect(),
            consumes: s.consumes.iter().map(|x| x.to_string()).collect(),
            adapters: s.adapters.iter().map(|x| x.to_string()).collect(),
            runtime: s.runtime.iter().map(|x| x.to_string()).collect(),
            dependencies: s.dependencies.iter().map(|x| x.to_string()).collect(),
            permissions: s.permissions.iter().map(|x| x.to_string()).collect(),
            compatibility: Compatibility {
                ir: Some(">=1.0.0 <2.0.0".to_string()),
                runtime: Some(">=1.0.0".to_string()),
            },
            conformance: Some(Conformance {
                suite: Some(s.suite.to_string()),
            }),
            deprecated: None,
        },
    }
}

/// Forma 自带的能力清单（与 `docs/architecture/capability/registry.md` 保持一致）。
pub fn builtin_manifests() -> Vec<CapabilityManifest> {
    vec![
        manifest(BuiltinSpec {
            id: "data.binding",
            version: "1.0.0",
            description: "数据绑定契约：把 API 结果绑定为可视化/表格的数据源",
            category: "data",
            provides: &["data.binding"],
            consumes: &["ApiIR", "EntityIR"],
            adapters: &["binding-adapter"],
            runtime: &["forma-web-runtime"],
            dependencies: &[],
            permissions: &["data.read"],
            suite: "data.binding.v1",
        }),
        manifest(BuiltinSpec {
            id: "data.mysql",
            version: "1.0.0",
            description: "MySQL 8 数据平面：DDL、版本化迁移与确定性种子",
            category: "data",
            provides: &["db.ddl", "db.migration", "db.seed"],
            consumes: &["EntityIR"],
            adapters: &["mysql-adapter"],
            runtime: &["forma-backend-runtime"],
            dependencies: &[],
            permissions: &["data.write"],
            suite: "data.mysql.v1",
        }),
        manifest(BuiltinSpec {
            id: "visualization.echarts",
            version: "1.0.0",
            description: "ECharts 渲染：Chart IR → option 纯函数投影 + React 运行时",
            category: "visualization",
            provides: &[
                "chart.bar",
                "chart.line",
                "chart.area",
                "chart.pie",
                "chart.scatter",
                "chart.funnel",
                "chart.gauge",
                "chart.radar",
                "chart.heatmap",
                "chart.treemap",
            ],
            consumes: &["ChartIR", "DashboardIR", "DataBinding"],
            adapters: &["echarts-adapter"],
            runtime: &["forma-web-runtime"],
            dependencies: &["data.binding"],
            permissions: &["data.read"],
            suite: "visualization.chart.v1",
        }),
        manifest(BuiltinSpec {
            id: "ui.form",
            version: "1.0.0",
            description: "表单原语：规则表达式 → zod / TanStack Form 校验",
            category: "ui",
            provides: &["ui.form"],
            consumes: &["PageIR", "RuleIR"],
            adapters: &["web-react-adapter"],
            runtime: &["forma-web-runtime"],
            dependencies: &[],
            permissions: &["data.read"],
            suite: "ui.form.v1",
        }),
        manifest(BuiltinSpec {
            id: "ui.table",
            version: "1.0.0",
            description: "表格原语：列定义、过滤、排序、分页",
            category: "ui",
            provides: &["ui.table"],
            consumes: &["PageIR", "ApiIR"],
            adapters: &["web-react-adapter"],
            runtime: &["forma-web-runtime"],
            dependencies: &[],
            permissions: &["data.read"],
            suite: "ui.table.v1",
        }),
        manifest(BuiltinSpec {
            id: "ui.dialog",
            version: "1.0.0",
            description: "对话框原语：命令类操作的确认与表单承载",
            category: "ui",
            provides: &["ui.dialog"],
            consumes: &["PageIR"],
            adapters: &["web-react-adapter"],
            runtime: &["forma-web-runtime"],
            dependencies: &[],
            permissions: &["data.read"],
            suite: "ui.dialog.v1",
        }),
        manifest(BuiltinSpec {
            id: "ui.navigation",
            version: "1.0.0",
            description: "导航原语：由 page.route 生成类型安全路由树",
            category: "ui",
            provides: &["ui.navigation"],
            consumes: &["PageIR"],
            adapters: &["web-react-adapter"],
            runtime: &["forma-web-runtime"],
            dependencies: &[],
            permissions: &[],
            suite: "ui.navigation.v1",
        }),
        manifest(BuiltinSpec {
            id: "integration.openapi",
            version: "1.0.0",
            description: "OpenAPI 3.1 导入/导出",
            category: "integrations",
            provides: &["integration.openapi"],
            consumes: &["EntityIR", "ApiIR"],
            adapters: &["openapi-adapter"],
            runtime: &["forma-cli"],
            dependencies: &[],
            permissions: &[],
            suite: "integration.openapi.v1",
        }),
        manifest(BuiltinSpec {
            id: "integration.jsonschema",
            version: "1.0.0",
            description: "JSON Schema 导入/导出",
            category: "integrations",
            provides: &["integration.jsonschema"],
            consumes: &["EntityIR"],
            adapters: &["jsonschema-adapter"],
            runtime: &["forma-cli"],
            dependencies: &[],
            permissions: &[],
            suite: "integration.jsonschema.v1",
        }),
        manifest(BuiltinSpec {
            id: "integration.sql",
            version: "1.0.0",
            description: "SQL DDL 导入/导出",
            category: "integrations",
            provides: &["integration.sql"],
            consumes: &["EntityIR"],
            adapters: &["sql-adapter"],
            runtime: &["forma-cli"],
            dependencies: &[],
            permissions: &[],
            suite: "integration.sql.v1",
        }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Version {
        Version::parse(s).expect("semver")
    }

    #[test]
    fn version_ranges_match() {
        assert!(version_matches(">=1.0.0 <2.0.0", &v("1.0.0")));
        assert!(version_matches(">=1.0.0 <2.0.0", &v("1.9.9")));
        assert!(!version_matches(">=1.0.0 <2.0.0", &v("2.0.0")));
        assert!(version_matches("", &v("3.2.1")));
        assert!(version_matches("^1.2.0", &v("1.9.0")));
        assert!(!version_matches("^1.2.0", &v("2.0.0")));
        assert!(version_matches("~1.2.0", &v("1.2.9")));
        assert!(!version_matches("~1.2.0", &v("1.3.0")));
        assert!(version_matches("=1.0.0", &v("1.0.0")));
        assert!(!version_matches("=1.0.0", &v("1.0.1")));
    }

    #[test]
    fn builtin_registry_resolves_dependency_closure() {
        let reg = CapabilityRegistry::with_builtins();
        let res = reg.resolve(&["visualization.echarts".to_string()], &[]);
        assert!(
            res.errors().is_empty(),
            "unexpected errors: {:?}",
            res.errors()
        );
        let ids: Vec<&str> = res.active.iter().map(|c| c.id.as_str()).collect();
        // 依赖在前：data.binding → visualization.echarts
        assert_eq!(ids, vec!["data.binding", "visualization.echarts"]);
        assert!(res.permissions.contains(&"data.read".to_string()));
    }

    #[test]
    fn unknown_capability_is_error() {
        let reg = CapabilityRegistry::with_builtins();
        let res = reg.resolve(&["nope.missing".to_string()], &[]);
        assert!(res.errors().iter().any(|d| d.code == "capability-unknown"));
    }

    #[test]
    fn pin_mismatch_is_error() {
        let reg = CapabilityRegistry::with_builtins();
        let res = reg.resolve(
            &["data.mysql".to_string()],
            &[("data.mysql".to_string(), "2.0.0".to_string())],
        );
        assert!(res
            .errors()
            .iter()
            .any(|d| d.code == "capability-pin-mismatch"));
    }
}
