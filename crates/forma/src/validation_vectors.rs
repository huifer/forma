//! 规则一致性向量（F-004 / P1-3）：从 DSL 规则**派生**跨端行为测试用例。
//!
//! 单一事实来源是 **DSL 本身**：对每个（实体 × 字段 × 约束）派生边界用例
//! （低于下界 / 下界 / 超过上界 / 上界 / 必填缺省），由 Web（vitest）与
//! Flutter（flutter_test）两个生成器渲染为**各自语言**的行为测试文件——
//! 两个测试文件由同一派生函数产出，因此天然同源；任何一端的校验器漂移
//! 都会在对应 CI 的真实执行中变红。
//!
//! 语义对齐（与三端校验器一致，见 spec:types / 生成器实现）：
//! - 数值边界仅对 Int / Float / Decimal 有意义（`NUMERIC_TYPES`）；
//! - 必填校验覆盖非可空的 String / Email / Text（`trim().isEmpty()`）；
//! - Decimal 在 TS / Dart 输入侧是**字符串**（`Number(input.x)` / `num.tryParse`），
//!   字面量按 scale 渲染；
//! - 聚合 / 关联 / 时间字段不参与边界向量。

use crate::dsl::{has_attr, is_enum, is_relation, FieldDecl, Program, NUMERIC_TYPES};

/// 一条行为用例的类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseKind {
    /// 低于下界 → 必须拒绝
    MinBelow,
    /// 恰在下界 → 必须通过
    MinBoundary,
    /// 超过上界 → 必须拒绝
    MaxAbove,
    /// 恰在上界 → 必须通过
    MaxBoundary,
    /// 必填字段空串 → 必须拒绝
    RequiredEmpty,
}

impl CaseKind {
    pub fn describe(self, boundary: &str) -> String {
        match self {
            CaseKind::MinBelow => format!("低于下界（{boundary}）→ 拒绝"),
            CaseKind::MinBoundary => format!("边界值（{boundary}）→ 通过"),
            CaseKind::MaxAbove => format!("超过上界（{boundary}）→ 拒绝"),
            CaseKind::MaxBoundary => format!("边界值（{boundary}）→ 通过"),
            CaseKind::RequiredEmpty => "必填缺省（空串）→ 拒绝".to_string(),
        }
    }
}

/// 一个用例的值（Str = 字符串字面量；Num = 数值字面量）。
#[derive(Debug, Clone)]
pub enum CaseValue {
    Str(String),
    Num(f64),
}

/// 一条行为用例。
#[derive(Debug, Clone)]
pub struct BehaviorCase {
    pub entity: String,
    pub field: String,
    pub kind: CaseKind,
    pub value: CaseValue,
    /// 边界的展示形式（用例描述用）
    pub boundary_display: String,
}

/// Decimal 的小数位（type_args 第 2 个；缺省 2）。
fn decimal_scale(field: &FieldDecl) -> usize {
    field
        .type_args
        .get(1)
        .and_then(|a| a.parse::<usize>().ok())
        .unwrap_or(2)
}

fn is_int(t: &str) -> bool {
    t == "Int"
}

fn is_decimal(t: &str) -> bool {
    t == "Decimal"
}

fn is_numeric(t: &str) -> bool {
    NUMERIC_TYPES.contains(&t)
}

/// 汇集一个字段上的 min/max 约束值（@min/@max 属性 + rules field 约束，去重）。
fn field_bounds(program: &Program, entity: &str, field: &FieldDecl) -> (Vec<f64>, Vec<f64>) {
    let mut mins: Vec<f64> = Vec::new();
    let mut maxs: Vec<f64> = Vec::new();
    let push = |v: f64, mins: &mut Vec<f64>, maxs: &mut Vec<f64>, is_min: bool| {
        if is_min {
            if !mins.contains(&v) {
                mins.push(v);
            }
        } else if !maxs.contains(&v) {
            maxs.push(v);
        }
    };
    for attr in &field.attrs {
        if matches!(attr.name.as_str(), "min" | "max") {
            if let Some(v) = attr.args.first().and_then(|a| a.parse::<f64>().ok()) {
                push(v, &mut mins, &mut maxs, attr.name == "min");
            }
        }
    }
    for rules in &program.rules {
        for fr in &rules.field_rules {
            if fr.entity != entity || fr.field != field.name {
                continue;
            }
            for c in &fr.constraints {
                match c {
                    crate::dsl::FieldConstraint::Min(v, _) => push(*v, &mut mins, &mut maxs, true),
                    crate::dsl::FieldConstraint::Max(v, _) => push(*v, &mut mins, &mut maxs, false),
                    _ => {}
                }
            }
        }
    }
    (mins, maxs)
}

/// 派生一个实体的行为用例（校验器存在 = api 声明了 create）。
pub fn behavior_cases(program: &Program, entity_name: &str) -> Vec<BehaviorCase> {
    let Some(api) = program.apis.iter().find(|a| a.entity == entity_name) else {
        return vec![];
    };
    if !api.operations.iter().any(|o| o == "create") {
        return vec![]; // 没有 create 校验器就没有行为向量
    }
    let Some(entity) = program.entities.iter().find(|e| e.name == entity_name) else {
        return vec![];
    };
    let mut out = Vec::new();
    for field in &entity.fields {
        if is_relation(program, &field.type_name) || is_enum(program, &field.type_name) {
            continue; // 关联 / 枚举无边界向量（枚举走词表校验，结构性断言覆盖）
        }
        let t = field.type_name.as_str();
        if is_numeric(t) {
            let scale = if is_decimal(t) {
                decimal_scale(field)
            } else {
                0
            };
            let eps = if is_int(t) {
                1.0
            } else if is_decimal(t) {
                10f64.powi(-(scale as i32))
            } else {
                1.0
            };
            let (mins, maxs) = field_bounds(program, entity_name, field);
            for m in &mins {
                out.push(BehaviorCase {
                    entity: entity_name.to_string(),
                    field: field.name.clone(),
                    kind: CaseKind::MinBelow,
                    value: CaseValue::Num(m - eps),
                    boundary_display: render_num(*m, is_int(t), is_decimal(t), scale),
                });
                out.push(BehaviorCase {
                    entity: entity_name.to_string(),
                    field: field.name.clone(),
                    kind: CaseKind::MinBoundary,
                    value: CaseValue::Num(*m),
                    boundary_display: render_num(*m, is_int(t), is_decimal(t), scale),
                });
            }
            for m in &maxs {
                out.push(BehaviorCase {
                    entity: entity_name.to_string(),
                    field: field.name.clone(),
                    kind: CaseKind::MaxAbove,
                    value: CaseValue::Num(m + eps),
                    boundary_display: render_num(*m, is_int(t), is_decimal(t), scale),
                });
                out.push(BehaviorCase {
                    entity: entity_name.to_string(),
                    field: field.name.clone(),
                    kind: CaseKind::MaxBoundary,
                    value: CaseValue::Num(*m),
                    boundary_display: render_num(*m, is_int(t), is_decimal(t), scale),
                });
            }
        } else if matches!(t, "String" | "Email" | "Text") && !field.optional {
            // 非可空字符串族 → 必填校验（trim().isEmpty）
            out.push(BehaviorCase {
                entity: entity_name.to_string(),
                field: field.name.clone(),
                kind: CaseKind::RequiredEmpty,
                value: CaseValue::Str(String::new()),
                boundary_display: String::new(),
            });
        }
    }
    out
}

fn render_num(v: f64, int: bool, decimal: bool, scale: usize) -> String {
    if int {
        format!("{}", v as i64)
    } else if decimal {
        format!("{v:.scale$}")
    } else {
        format!("{v}")
    }
}

// ── 输入键与字面量渲染（与各端 Input 类型严格同形）────────────────

/// 输入对象的字段键：TS 用 snake_case（与 types.ts Create* 一致），Dart 用 lowerCamelCase。
pub fn input_key(f: &FieldDecl, dart: bool) -> String {
    let key = crate::identity::to_snake_case(&f.name);
    if dart {
        crate::identity::to_camel_case(&key)
    } else {
        key
    }
}

/// 基准输入值（构造一个**完全合法**的输入体；可选字段渲染为 null/省略标记由调用方处理）。
pub fn base_value(program: &Program, field: &FieldDecl, dart: bool) -> Option<String> {
    if is_relation(program, &field.type_name) {
        return Some("\"00000000-0000-5000-8000-000000000000\"".to_string());
    }
    if is_enum(program, &field.type_name) {
        let first = program
            .enums
            .iter()
            .find(|e| e.name == field.type_name)
            .and_then(|e| e.variants.first())
            .map(|v| v.name.clone())
            .unwrap_or_default();
        return Some(if dart {
            format!("{}.{}", field.type_name, first)
        } else {
            format!("\"{first}\"")
        });
    }
    match field.type_name.as_str() {
        "String" | "Email" | "Text" => Some("\"vector\"".to_string()),
        "Int" => Some("1".to_string()),
        "Float" => Some("1.5".to_string()),
        "Decimal" => Some("\"1.00\"".to_string()),
        "Bool" => Some("true".to_string()),
        "Timestamp" | "Date" => {
            let iso = if field.type_name == "Date" {
                "2025-01-06"
            } else {
                "2025-01-06T08:00:00Z"
            };
            Some(if dart {
                format!("DateTime.parse(\"{iso}\")")
            } else {
                format!("\"{iso}\"")
            })
        }
        "Json" => Some(if dart {
            "const {}".to_string()
        } else {
            "{}".to_string()
        }),
        _ => None,
    }
}

/// 用例值的字面量（按字段类型渲染；Decimal 在 TS/Dart 输入侧是字符串）。
pub fn case_literal(field: &FieldDecl, value: &CaseValue) -> String {
    match value {
        CaseValue::Str(s) => format!("\"{}\"", s.replace('"', "\\\"")),
        CaseValue::Num(v) => {
            let t = field.type_name.as_str();
            if is_decimal(t) {
                let scale = decimal_scale(field);
                format!("\"{v:.scale$}\"")
            } else if is_int(t) {
                format!("{}", *v as i64)
            } else {
                format!("{v}")
            }
        }
    }
}

/// 可空标记（可选字段在 Dart 构造器里可省参；TS 里可传 null）。
pub fn is_optional_input(program: &Program, field: &FieldDecl) -> bool {
    if is_relation(program, &field.type_name) {
        return field.optional;
    }
    field.optional || has_attr(field, "default") || has_attr(field, "readonly")
}
