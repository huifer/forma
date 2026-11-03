//! mobile-flutter 适配层（gen-flutter）：Program → Flutter/Dart 客户端（Android + iOS 一套代码）。
//!
//! 与 gen-web-react 同构的四件套：
//! - models（对应 types.ts）：实体类 + 枚举 + ListRow + Input，fromJson/toJson
//!   键名与 Rust 后端 serde 序列化严格一致（实体/ListRow 用 `{rel}_id`，
//!   Create/Update 输入用裸关系名 `{rel}`——这是后端真实契约，两处不同）。
//! - api（对应 api.ts）：http 客户端 + 每实体 CRUD + ListParams，
//!   路由 `/api/{plural_snake}`、查询参数 page/page_size/q/filters(+_contains)。
//! - validation（对应 validators.ts）：同一套 DSL rules 三端同源编译
//!   （Rust 权威 + Web zod + Dart 表单校验）。
//! - pages（对应 list/edit page）：页面原语 → Material 组件。
//!
//! 生成物是普通 Flutter 工程：`flutter pub get && flutter run` 即可运行；
//! Android 模拟器默认 API_BASE=10.0.2.2（宿主回环），iOS 用 --dart-define 覆盖。

use super::{
    find_entity, has_attr, is_enum, is_relation, pick_message, EntityDecl, FieldDecl, Program,
};
use crate::dsl::{ApiDecl, DetailCfg, FormCfg, PageBody, PageDecl, TableCfg};
use crate::identity::to_snake_case;
use std::path::Path;

pub fn emit_flutter(
    project_dir: &Path,
    program: &Program,
    out_dir: &str,
    app_name: &str,
) -> anyhow::Result<Vec<crate::generators::GeneratedFile>> {
    if program.entities.is_empty() {
        return Ok(vec![]);
    }
    let pkg = to_snake_case(app_name);
    let version = program
        .app
        .as_ref()
        .map(|a| a.version.clone())
        .unwrap_or_else(|| "0.0.0".into());

    let mut files = vec![file(out_dir, "pubspec.yaml", pubspec(&pkg, &version))];
    files.push(file(out_dir, "analysis_options.yaml", analysis_options()));
    files.push(file(
        out_dir,
        "lib/models/models.dart",
        models_dart(program),
    ));
    files.push(file(out_dir, "lib/api/api.dart", api_dart(program)));
    files.push(file(
        out_dir,
        "lib/validation/validators.dart",
        validators_dart(program),
    ));
    files.push(file(out_dir, "lib/ui/format.dart", format_dart()));
    for page in &program.pages {
        let rel = format!("lib/pages/{}.dart", to_snake_case(&page.name));
        let content = match &page.body {
            PageBody::Table(cfg) => list_page_dart(program, page, cfg),
            PageBody::Form(cfg) => edit_page_dart(program, page, cfg),
            PageBody::Detail(cfg) => detail_page_dart(program, page, cfg),
        };
        files.push(file(out_dir, &rel, content));
    }
    files.push(file(out_dir, "lib/main.dart", main_dart(program, &pkg)));
    files.push(file(
        out_dir,
        "test/live_api_test.dart",
        live_api_test_dart(program, &pkg),
    ));
    files.push(file(
        out_dir,
        "README.md",
        readme_md(program, app_name, &version),
    ));

    for f in &files {
        crate::util::write_text(&project_dir.join(&f.path), &f.content)?;
    }
    Ok(files)
}

fn file(out_dir: &str, rel: &str, content: String) -> crate::generators::GeneratedFile {
    crate::generators::GeneratedFile {
        path: format!("{out_dir}/{rel}"),
        content,
        generator: "gen-flutter".into(),
    }
}

// ── 通用小件 ────────────────────────────────────────────

fn escape_dart(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace('$', r"\$")
        .replace('\n', "\\n")
}

/// 与 gen-web 的 humanize 一致：snake_case → 分词首字母大写。
fn humanize(name: &str) -> String {
    let mut out = String::new();
    let mut prev_sep = true;
    for c in name.chars() {
        if c == '_' || c == '-' || c == ' ' {
            prev_sep = true;
            continue;
        }
        if prev_sep {
            out.extend(c.to_uppercase());
            prev_sep = false;
        } else {
            out.push(c);
        }
    }
    out
}
/// snake_case → lowerCamelCase（Dart 标识符规范；JSON 键仍保持 snake_case）
fn to_camel(name: &str) -> String {
    let mut out = String::new();
    let mut upper_next = false;
    for c in name.chars() {
        if c == '_' || c == '-' {
            upper_next = true;
        } else if upper_next {
            out.extend(c.to_uppercase());
            upper_next = false;
        } else {
            out.push(c);
        }
    }
    out
}

fn plural_snake(entity: &str) -> String {
    to_snake_case(&super::pluralize(entity))
}

fn dart_type(program: &Program, f: &FieldDecl) -> String {
    let base = match f.type_name.as_str() {
        "Uuid" | "String" | "Email" | "Text" | "Decimal" => "String".to_string(),
        "Int" => "int".to_string(),
        "Float" => "double".to_string(),
        "Bool" => "bool".to_string(),
        "Timestamp" | "Date" => "DateTime".to_string(),
        "Json" => "Map<String, dynamic>".to_string(),
        other => {
            if is_enum(program, other) {
                other.to_string()
            } else {
                "String".to_string() // 关系 → FK
            }
        }
    };
    base
}

fn is_option_field(f: &FieldDecl) -> bool {
    f.optional || has_attr(f, "default") || has_attr(f, "readonly")
}

/// 实体展示字段：第一个 required 的 String/Email，否则第一个 String，否则 id。
fn display_field_of(entity: &EntityDecl) -> String {
    for f in &entity.fields {
        if matches!(f.type_name.as_str(), "String" | "Email") && has_attr(f, "required") {
            return to_snake_case(&f.name);
        }
    }
    for f in &entity.fields {
        if f.type_name == "String" {
            return to_snake_case(&f.name);
        }
    }
    "id".into()
}

fn api_of<'a>(program: &'a Program, entity_name: &str) -> Option<&'a ApiDecl> {
    program.apis.iter().find(|a| a.entity == entity_name)
}

fn api_of_by_name<'a>(program: &'a Program, api_name: &str) -> Option<&'a ApiDecl> {
    program.apis.iter().find(|a| a.name == api_name)
}

fn num_literal(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

// ── pubspec / analysis_options / README ─────────────────

fn pubspec(pkg: &str, version: &str) -> String {
    format!(
        r#"# @generated by Forma (gen-flutter) — do not edit manually.
name: {pkg}
description: Forma 生成的 Flutter 客户端（Android / iOS 同源），与 Rust 后端、Web 共用同一套 DSL 契约。
publish_to: none
version: {version}

environment:
  sdk: ">=3.5.0 <4.0.0"

dependencies:
  flutter:
    sdk: flutter
  http: ^1.2.0

dev_dependencies:
  flutter_test:
    sdk: flutter
  flutter_lints: ">=4.0.0"

flutter:
  uses-material-design: true
"#
    )
}

fn analysis_options() -> String {
    "include: package:flutter_lints/flutter.yaml\n".to_string()
}

fn readme_md(program: &Program, app_name: &str, version: &str) -> String {
    let first_table = program
        .pages
        .iter()
        .find(|p| matches!(p.body, PageBody::Table(_)));
    let list_route = first_table
        .map(|p| p.route.clone())
        .unwrap_or_else(|| "/".into());
    let pkg = to_snake_case(app_name);
    format!(
        r#"# {app_name} · Flutter 客户端（Android / iOS）

由 Forma `gen-flutter` 生成 —— 与 Rust 后端（axum）、React Web 共用同一套 DSL 契约：
模型、REST API、校验规则三端同源，改 `.fm` 后 `forma build` 全部重生成。

## 结构

```
lib/
  models/models.dart        # 实体 + 枚举 + ListRow + Input（fromJson/toJson，键名=后端 serde）
  api/api.dart              # REST 客户端：/api/{{plural}}，page/page_size/q/filters
  validation/validators.dart# DSL rules 的 Dart 编译（与 Rust validators.rs 同源）
  ui/format.dart            # 日期/数字展示与输入格式
  pages/*.dart              # 列表页 + 编辑页（页面原语 → Material）
  main.dart                 # 路由表 + {{id}} 参数路由匹配
```

## 运行

```bash
# 1) 准备数据库并导入种子数据（案例数据由 gen-seed 确定性生成）
mysql -u{{PASSWORD_583ugbf8}} -p -e "CREATE DATABASE IF NOT EXISTS {pkg}"
mysql -u{{PASSWORD_583ugbf8}} -p {pkg} < ../db/seed/seed_{version}.sql

# 2) 启动后端（默认 0.0.0.0:8080）
cd ../generated/backend
DATABASE_URL="mysql://{{PASSWORD_583ugbf8}}:{{PASSWORD_583ugbf8}}@127.0.0.1:3306/{pkg}" cargo run

# 3) 运行 App
flutter pub get
flutter run                                                    # Android 模拟器（默认 API_BASE=http://10.0.2.2:8080/api）
flutter run --dart-define=API_BASE=http://127.0.0.1:8080/api   # iOS 模拟器 / 真机按局域网 IP
```

入口路由：`{list_route}`。API 地址一律通过 `--dart-define=API_BASE=...` 覆盖。

## 要求

- Flutter ≥ 3.35（使用 `DropdownButtonFormField.initialValue`）
- 后端由同一 `forma.yaml` 的 `backend` target 生成，CORS 已放行
"#
    )
}

// ── models.dart ─────────────────────────────────────────

fn models_dart(program: &Program) -> String {
    let mut out = String::from(
        "// @generated by Forma (gen-flutter) — do not edit manually.\n// 与 Rust 后端 serde / Web types.ts 同源：snake_case 键（Dart 字段名为 camelCase）、Timestamp=ISO8601、Decimal=string。\n\n",
    );
    let has_nullable_dt = program.entities.iter().any(|e| {
        e.fields
            .iter()
            .any(|f| f.optional && matches!(f.type_name.as_str(), "Timestamp" | "Date"))
    });
    if has_nullable_dt {
        out.push_str(
            "DateTime? _dt(dynamic v) => v == null ? null : DateTime.tryParse(v.toString());\n\n",
        );
    }

    for e in &program.enums {
        let variants = e
            .variants
            .iter()
            .map(|v| format!("  {},", v.name))
            .collect::<Vec<_>>()
            .join("\n");
        out.push_str(&format!("enum {} {{\n{}\n}}\n\n", e.name, variants));

        let label_cases = e
            .variants
            .iter()
            .map(|v| {
                let label = pick_message(&v.labels, &v.name);
                format!(
                    "    case {}.{}:\n      return '{}';",
                    e.name,
                    v.name,
                    escape_dart(&label)
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        out.push_str(&format!(
            "extension {}Label on {} {{\n  /// DSL @label（zh → en → 原名）\n  String get label {{\n    switch (this) {{\n{}\n    }}\n  }}\n}}\n\n",
            e.name, e.name, label_cases
        ));

        let mut from = format!(
            "{}? {}FromName(String? name) {{\n  switch (name) {{\n",
            e.name,
            to_camel(&to_snake_case(&e.name))
        );
        for v in &e.variants {
            from.push_str(&format!(
                "    case '{}':\n      return {}.{};\n",
                v.name, e.name, v.name
            ));
        }
        from.push_str("    default:\n      return null;\n  }\n}\n\n");
        out.push_str(&from);
    }

    for entity in &program.entities {
        out.push_str(&entity_class_dart(program, entity));
        out.push('\n');
        out.push_str(&list_row_class_dart(program, entity));
        out.push('\n');
        out.push_str(&input_class_dart(program, entity));
        out.push('\n');
    }
    out
}

fn entity_class_dart(program: &Program, entity: &EntityDecl) -> String {
    let name = &entity.name;
    let mut decl = String::new();
    let mut ctor = String::new();
    let mut from = String::new();
    let mut tojson = String::new();
    for f in &entity.fields {
        let key = json_key(program, f);
        let fname = to_camel(&key);
        let t = dart_type(program, f);
        if f.optional {
            decl.push_str(&format!("  final {t}? {fname};\n"));
            ctor.push_str(&format!("    this.{fname},\n"));
        } else {
            decl.push_str(&format!("  final {t} {fname};\n"));
            ctor.push_str(&format!("    required this.{fname},\n"));
        }
        from.push_str(&format!(
            "    {fname}: {},\n",
            json_read_expr(program, f, &key)
        ));
        tojson.push_str(&format!(
            "    '{key}': {},\n",
            json_write_expr(program, f, &fname)
        ));
    }
    format!(
        "class {name} {{\n{decl}\n  const {name}({{\n{ctor}  }});\n\n  factory {name}.fromJson(Map<String, dynamic> j) => {name}(\n{from}  );\n\n  Map<String, dynamic> toJson() => {{\n{tojson}  }};\n}}\n",
        name = name,
        decl = decl,
        ctor = ctor,
        from = from,
        tojson = tojson,
    )
}

fn list_row_class_dart(program: &Program, entity: &EntityDecl) -> String {
    let name = &entity.name;
    let mut decl = String::new();
    let mut ctor = String::new();
    let mut from = String::new();
    for f in &entity.fields {
        let key = json_key(program, f);
        let fname = to_camel(&key);
        if f.optional {
            ctor.push_str(&format!("    super.{fname},\n"));
        } else {
            ctor.push_str(&format!("    required super.{fname},\n"));
        }
        from.push_str(&format!(
            "    {fname}: {},\n",
            json_read_expr(program, f, &key)
        ));
        if is_relation(program, &f.type_name) {
            let name_key = format!("{}_name", to_snake_case(&f.name));
            let fname2 = to_camel(&name_key);
            decl.push_str(&format!("  final String? {fname2};\n"));
            ctor.push_str(&format!("    required this.{fname2},\n"));
            from.push_str(&format!("    {fname2}: j['{name_key}'] as String?,\n"));
        }
    }
    format!(
        "/// 列表行：与后端 {name}ListRow 一致，关系字段带 *_name 显示列。\nclass {name}ListRow extends {name} {{\n{decl}\n  const {name}ListRow({{\n{ctor}  }});\n\n  factory {name}ListRow.fromJson(Map<String, dynamic> j) => {name}ListRow(\n{from}  );\n}}\n",
        name = name,
        decl = decl,
        ctor = ctor,
        from = from,
    )
}

fn input_class_dart(program: &Program, entity: &EntityDecl) -> String {
    let name = &entity.name;
    let mut decl = String::new();
    let mut ctor = String::new();
    let mut tojson = String::new();
    for f in &entity.fields {
        if has_attr(f, "id") {
            continue; // id 由服务端生成（update 走路径参数）
        }
        // 后端 Create/Update 输入的关系字段用裸关系名（school），不是 school_id
        let key = to_snake_case(&f.name);
        let fname = to_camel(&key);
        let t = if is_relation(program, &f.type_name) {
            "String".to_string()
        } else {
            dart_type(program, f)
        };
        let opt = is_option_field(f);
        if opt {
            decl.push_str(&format!("  final {t}? {fname};\n"));
            ctor.push_str(&format!("    this.{fname},\n"));
        } else {
            decl.push_str(&format!("  final {t} {fname};\n"));
            ctor.push_str(&format!("    required this.{fname},\n"));
        }
        let value_expr = match (opt, f.type_name.as_str()) {
            (true, "Timestamp") => format!("{fname}!.toIso8601String()"),
            (true, "Date") => format!("{fname}!.toIso8601String().substring(0, 10)"),
            (true, other) if is_enum(program, other) => format!("{fname}!.name"),
            (true, _) => fname.clone(),
            (false, "Timestamp") => format!("{fname}.toIso8601String()"),
            (false, "Date") => format!("{fname}.toIso8601String().substring(0, 10)"),
            (false, other) if is_enum(program, other) => format!("{fname}.name"),
            (false, _) => fname.clone(),
        };
        if opt {
            tojson.push_str(&format!(
                "    if ({fname} != null) '{key}': {value_expr},\n"
            ));
        } else {
            tojson.push_str(&format!("    '{key}': {value_expr},\n"));
        }
    }
    format!(
        "/// Create/Update 输入体：键名与后端 Create{name}/Update{name} serde 一致。\n/// readonly 字段（如 createdAt）在 update 时回传，供 check 规则引用。\nclass {name}Input {{\n{decl}\n  const {name}Input({{\n{ctor}  }});\n\n  Map<String, dynamic> toJson() => {{\n{tojson}  }};\n}}\n",
        name = name,
        decl = decl,
        ctor = ctor,
        tojson = tojson,
    )
}

/// 实体/ListRow JSON 的键名：关系字段 = `{rel}_id`。
fn json_key(program: &Program, f: &FieldDecl) -> String {
    if is_relation(program, &f.type_name) {
        format!("{}_id", to_snake_case(&f.name))
    } else {
        to_snake_case(&f.name)
    }
}

fn json_read_expr(program: &Program, f: &FieldDecl, key: &str) -> String {
    let opt = f.optional;
    let access = format!("j['{key}']");
    match f.type_name.as_str() {
        "Int" if opt => format!("({access} as num?)?.toInt()"),
        "Int" => format!("({access} as num).toInt()"),
        "Float" if opt => format!("({access} as num?)?.toDouble()"),
        "Float" => format!("({access} as num).toDouble()"),
        "Timestamp" | "Date" if opt => format!("_dt({access})"),
        "Timestamp" | "Date" => format!("DateTime.parse({access} as String)"),
        "Bool" if opt => format!("{access} as bool?"),
        "Bool" => format!("{access} as bool"),
        "Json" if opt => format!("{access} as Map<String, dynamic>?"),
        "Json" => format!("{access} as Map<String, dynamic>"),
        other if is_enum(program, other) => {
            let parser = format!("{}FromName", to_camel(&to_snake_case(other)));
            if opt {
                format!("{parser}({access} as String?)")
            } else {
                format!("{parser}({access} as String?)!")
            }
        }
        _ if opt => format!("{access} as String?"),
        _ => format!("{access} as String"),
    }
}

fn json_write_expr(program: &Program, f: &FieldDecl, key: &str) -> String {
    match f.type_name.as_str() {
        "Timestamp" => format!("{key}.toIso8601String()"),
        "Date" => format!("{key}.toIso8601String().substring(0, 10)"),
        other if is_enum(program, other) => format!("{key}.name"),
        _ => key.to_string(),
    }
}

// ── api.dart ────────────────────────────────────────────

fn api_dart(program: &Program) -> String {
    let mut out = String::from(
        "// @generated by Forma (gen-flutter) — do not edit manually.\nimport 'dart:convert';\n\nimport 'package:http/http.dart' as http;\n\nimport '../models/models.dart';\n\n/// API 基址。Android 模拟器访问宿主机用 10.0.2.2；\n/// iOS 模拟器用 127.0.0.1；真机用局域网 IP。\n/// 覆盖方式：flutter run --dart-define=API_BASE=http://127.0.0.1:8080/api\nconst String kApiBase = String.fromEnvironment(\n  'API_BASE',\n  defaultValue: 'http://10.0.2.2:8080/api',\n);\n\nclass FieldError {\n  final String field;\n  final String message;\n  const FieldError(this.field, this.message);\n\n  static List<FieldError> fromJsonList(List<dynamic>? raw) =>\n      raw?.map((e) => FieldError(e['field'] as String, e['message'] as String)).toList() ??\n          const <FieldError>[];\n}\n\nclass ApiException implements Exception {\n  final int status;\n  final String message;\n  final List<FieldError> fields;\n  ApiException(this.status, this.message, this.fields);\n\n  @override\n  String toString() => 'ApiException($status): $message';\n}\n\nclass Api {\n  final http.Client client;\n  final String baseUrl;\n  Api({http.Client? client, String? baseUrl})\n      : client = client ?? http.Client(),\n        baseUrl = baseUrl ?? kApiBase;\n\n  Future<dynamic> _send(String method, Uri uri, {Map<String, dynamic>? body}) async {\n    final req = http.Request(method, uri);\n    if (body != null) {\n      req.headers['Content-Type'] = 'application/json';\n      req.body = jsonEncode(body);\n    }\n    final res = await http.Response.fromStream(await client.send(req));\n    if (res.statusCode >= 400) {\n      Map<String, dynamic> err = const {};\n      try {\n        final decoded = jsonDecode(res.body);\n        if (decoded is Map<String, dynamic>) err = decoded;\n      } catch (_) {\n        // 非 JSON 错误体（网关/代理）→ 保留状态码\n      }\n      throw ApiException(\n        res.statusCode,\n        (err['error'] as String?) ?? res.reasonPhrase ?? 'request failed',\n        FieldError.fromJsonList(err['fields'] as List<dynamic>?),\n      );\n    }\n    if (res.statusCode == 204 || res.body.isEmpty) return null;\n    return jsonDecode(res.body);\n  }\n\n  Future<List<T>> list<T>(\n      String path, T Function(Map<String, dynamic>) fromJson, Map<String, String> query) async {\n    final uri = Uri.parse('$baseUrl$path')\n        .replace(queryParameters: query.isEmpty ? null : query);\n    final data = await _send('GET', uri);\n    return (data as List<dynamic>)\n        .map((e) => fromJson(e as Map<String, dynamic>))\n        .toList();\n  }\n\n  Future<T> get<T>(String path, T Function(Map<String, dynamic>) fromJson) async {\n    final data = await _send('GET', Uri.parse('$baseUrl$path'));\n    return fromJson(data as Map<String, dynamic>);\n  }\n\n  Future<void> post(String path, Map<String, dynamic> body) async {\n    await _send('POST', Uri.parse('$baseUrl$path'), body: body);\n  }\n\n  Future<void> put(String path, Map<String, dynamic> body) async {\n    await _send('PUT', Uri.parse('$baseUrl$path'), body: body);\n  }\n\n  Future<void> delete(String path) async {\n    await _send('DELETE', Uri.parse('$baseUrl$path'));\n  }\n}\n\n",
    );

    for entity in &program.entities {
        let Some(api) = api_of(program, &entity.name) else {
            continue;
        };
        out.push_str(&params_class_dart(program, entity, api));
        out.push_str(&entity_api_class_dart(program, entity, api));
    }
    out
}

fn params_class_dart(program: &Program, entity: &EntityDecl, api: &ApiDecl) -> String {
    let name = &entity.name;
    let mut decl = String::new();
    let mut ctor = String::new();
    let mut query = String::new();

    decl.push_str("  final int? page;\n  final int? pageSize;\n  final String? q;\n");
    ctor.push_str("    this.page,\n    this.pageSize,\n    this.q,\n");
    query.push_str("    if (page != null) 'page': '$page',\n");
    query.push_str("    if (pageSize != null) 'page_size': '$pageSize',\n");
    query.push_str("    if (q != null && q!.isNotEmpty) 'q': q!,\n");

    if let Some(list) = &api.list {
        for (field, contains) in &list.filters {
            let pname = to_snake_case(field);
            let suffix = if *contains { "_contains" } else { "" };
            let ptype = filter_param_type(program, entity, field);
            let fname = to_camel(&format!("{pname}{suffix}"));
            decl.push_str(&format!("  final {ptype} {fname};\n"));
            ctor.push_str(&format!("    this.{fname},\n"));
            query.push_str(&format!(
                "    if ({fname} != null) '{pname}{suffix}': '${fname}',\n"
            ));
        }
    }

    format!(
        "class {name}ListParams {{\n{decl}\n  const {name}ListParams({{\n{ctor}  }});\n\n  Map<String, String> toQuery() => {{\n{query}  }};\n}}\n\n",
        name = name,
        decl = decl,
        ctor = ctor,
        query = query,
    )
}

fn filter_param_type(program: &Program, entity: &EntityDecl, field: &str) -> String {
    let Some(f) = entity.fields.iter().find(|f| f.name == *field) else {
        return "String?".into();
    };
    if is_relation(program, &f.type_name) {
        return "String?".into();
    }
    match f.type_name.as_str() {
        "Int" => "int?".into(),
        "Float" => "double?".into(),
        _ => "String?".into(),
    }
}

fn entity_api_class_dart(_program: &Program, entity: &EntityDecl, api: &ApiDecl) -> String {
    let name = &entity.name;
    let plural = plural_snake(&entity.name);
    let has = |op: &str| api.operations.iter().any(|o| o == op);

    let mut methods = String::new();
    if has("list") {
        methods.push_str(&format!(
            "  Future<List<{name}ListRow>> list({name}ListParams p) =>\n      client.list('/{plural}', {name}ListRow.fromJson, p.toQuery());\n"
            , plural = plural
        ));
    }
    if has("get") {
        methods.push_str(&format!(
            "  Future<{name}> get(String id) => client.get('/{plural}/$id', {name}.fromJson);\n"
        ));
    }
    if has("create") {
        methods.push_str(&format!(
            "  Future<void> create({name}Input input) => client.post('/{plural}', input.toJson());\n"
        ));
    }
    if has("update") {
        methods.push_str(&format!(
            "  Future<void> update(String id, {name}Input input) => client.put('/{plural}/$id', input.toJson());\n"
        ));
    }
    if has("delete") {
        methods.push_str(&format!(
            "  Future<void> remove(String id) => client.delete('/{plural}/$id');\n"
        ));
    }

    format!(
        "class {name}Api {{\n  final Api client;\n  {name}Api(this.client);\n\n{methods}}}\n\n",
        name = name,
        methods = methods,
    )
}

// ── validators.dart ─────────────────────────────────────

fn validators_dart(program: &Program) -> String {
    let mut out = String::from(
        "// @generated by Forma (gen-flutter) — do not edit manually.\n// 与后端 validators.rs / Web validators.ts 同源：同一套 DSL 规则三端编译。\n\nimport '../api/api.dart';\nimport '../models/models.dart';\n\n",
    );

    for entity in &program.entities {
        for is_create in [true, false] {
            let kind = if is_create { "Create" } else { "Update" };
            out.push_str(&format!(
                "List<FieldError> validate{}{}({}Input input) {{\n  final errs = <FieldError>[];\n",
                kind, entity.name, entity.name
            ));
            out.push_str(&field_checks_dart(program, entity));
            out.push_str(&check_rules_dart(program, entity));
            out.push_str("  return errs;\n}\n\n");
        }
    }
    out
}

fn field_checks_dart(program: &Program, entity: &EntityDecl) -> String {
    let mut out = String::new();
    let mut rules_by_field: std::collections::HashMap<String, Vec<&crate::dsl::FieldConstraint>> =
        Default::default();
    for rules in &program.rules {
        for fr in &rules.field_rules {
            if fr.entity == entity.name {
                rules_by_field
                    .entry(fr.field.clone())
                    .or_default()
                    .extend(fr.constraints.iter());
            }
        }
    }

    for field in &entity.fields {
        if has_attr(field, "id") {
            continue;
        }
        let key = to_snake_case(&field.name); // FieldError.field：与后端一致（snake）
        let col = to_camel(&key); // Input 字段访问（camel）
        let rel = is_relation(program, &field.type_name);
        let opt = is_option_field(field);
        let is_text = matches!(field.type_name.as_str(), "String" | "Email" | "Text");

        // required（默认仅非可空文本；显式 rules Required 覆盖）
        let mut required_msg: Option<String> = None;
        if !rel && !field.optional && !has_attr(field, "default") && is_text {
            required_msg = Some("必填 / required".into());
        }
        if let Some(constraints) = rules_by_field.get(&field.name) {
            for c in constraints {
                if let crate::dsl::FieldConstraint::Required(msg) = c {
                    required_msg = Some(pick_message(msg, "required"));
                }
            }
        }
        if let Some(msg) = required_msg {
            if rel {
                if opt {
                    out.push_str(&format!(
                        "  if (input.{col}?.isEmpty ?? true) errs.add(const FieldError('{key}', '{}'));\n",
                        escape_dart(&msg)
                    ));
                } else {
                    out.push_str(&format!(
                        "  if (input.{col}.isEmpty) errs.add(const FieldError('{key}', '{}'));\n",
                        escape_dart(&msg)
                    ));
                }
            } else if opt {
                out.push_str(&format!(
                    "  if (input.{col} != null && input.{col}!.trim().isEmpty) errs.add(const FieldError('{key}', '{}'));\n",
                    escape_dart(&msg)
                ));
            } else {
                out.push_str(&format!(
                    "  if (input.{col}.trim().isEmpty) errs.add(const FieldError('{key}', '{}'));\n",
                    escape_dart(&msg)
                ));
            }
        }

        // min/max（数值型：Int/Float/Decimal；枚举成员校验由 Dart 类型系统承担）
        let is_numeric = matches!(field.type_name.as_str(), "Int" | "Float" | "Decimal");
        if is_numeric {
            let (mins, maxs) = bounds_msgs(entity, field, &rules_by_field);
            for (v, msg) in mins {
                out.push_str(&numeric_check_dart(field, &key, &col, opt, v, "<", &msg));
            }
            for (v, msg) in maxs {
                out.push_str(&numeric_check_dart(field, &key, &col, opt, v, ">", &msg));
            }
        }
    }
    out
}

type BoundsMsgs = (Vec<(f64, String)>, Vec<(f64, String)>);

fn bounds_msgs(
    _entity: &EntityDecl,
    field: &FieldDecl,
    rules_by_field: &std::collections::HashMap<String, Vec<&crate::dsl::FieldConstraint>>,
) -> BoundsMsgs {
    let col = to_snake_case(&field.name);
    let mut mins = Vec::new();
    let mut maxs = Vec::new();
    for attr in &field.attrs {
        if matches!(attr.name.as_str(), "min" | "max") {
            if let Some(v) = attr.args.first().and_then(|a| a.parse::<f64>().ok()) {
                let msg = format!(
                    "{col} must be {} {}",
                    if attr.name == "min" { ">=" } else { "<=" },
                    num_literal(v)
                );
                if attr.name == "min" {
                    mins.push((v, msg));
                } else {
                    maxs.push((v, msg));
                }
            }
        }
    }
    if let Some(constraints) = rules_by_field.get(&field.name) {
        for c in constraints {
            match c {
                crate::dsl::FieldConstraint::Min(v, msg) => {
                    mins.push((*v, pick_message(msg, "too small")))
                }
                crate::dsl::FieldConstraint::Max(v, msg) => {
                    maxs.push((*v, pick_message(msg, "too large")))
                }
                _ => {}
            }
        }
    }
    (mins, maxs)
}

fn numeric_check_dart(
    field: &FieldDecl,
    key: &str,
    col: &str,
    opt: bool,
    v: f64,
    cmp: &str,
    msg: &str,
) -> String {
    let lit = num_literal(v);
    let cond = match field.type_name.as_str() {
        "Decimal" => {
            if opt {
                format!("input.{col} != null && (num.tryParse(input.{col}!) ?? 0) {cmp} {lit}")
            } else {
                format!("(num.tryParse(input.{col}) ?? 0) {cmp} {lit}")
            }
        }
        _ => {
            if opt {
                format!("input.{col} != null && input.{col}! {cmp} {lit}")
            } else {
                format!("input.{col} {cmp} {lit}")
            }
        }
    };
    format!(
        "  if ({cond}) errs.add(const FieldError('{key}', '{}'));\n",
        escape_dart(msg)
    )
}

fn check_rules_dart(program: &Program, entity: &EntityDecl) -> String {
    let mut out = String::new();
    for rules in &program.rules {
        for check in &rules.checks {
            if check.entity != entity.name {
                continue;
            }
            if expr_uses_old(&check.expr) {
                continue;
            }
            let cond = dart_expr(program, &check.expr, entity);
            let msg = pick_message(&check.message, "check failed");
            out.push_str(&format!(
                "  if (!({cond})) errs.add(const FieldError('__check', '{}'));\n",
                escape_dart(&msg)
            ));
        }
    }
    out
}

fn expr_uses_old(expr: &crate::dsl::Expr) -> bool {
    match expr {
        crate::dsl::Expr::Ref { old, .. } => *old,
        crate::dsl::Expr::List(items) => items.iter().any(expr_uses_old),
        crate::dsl::Expr::Call(_, args) => args.iter().any(expr_uses_old),
        crate::dsl::Expr::Not(inner) => expr_uses_old(inner),
        crate::dsl::Expr::Bin(_, a, b) => expr_uses_old(a) || expr_uses_old(b),
        _ => false,
    }
}

/// 表达式是否引用 Timestamp/Date 字段（DateTime 无原生比较运算符 → compareTo）
fn expr_is_ts(e: &crate::dsl::Expr, entity: &EntityDecl) -> bool {
    match e {
        crate::dsl::Expr::Ref { path, old } => {
            if *old {
                return false;
            }
            entity
                .fields
                .iter()
                .find(|f| f.name == path[0])
                .map(|f| matches!(f.type_name.as_str(), "Timestamp" | "Date"))
                .unwrap_or(false)
        }
        _ => false,
    }
}

/// check 规则 → Dart 表达式。old./关系显示列引用在类型化 Input 上不可用 →
/// 跳过（交后端权威校验），与 Web 端 `old → true` 的处理同语义。
fn dart_expr(program: &Program, expr: &crate::dsl::Expr, entity: &EntityDecl) -> String {
    match expr {
        crate::dsl::Expr::Num(v) => num_literal(*v),
        crate::dsl::Expr::Str(s) => format!("'{}'", escape_dart(s)),
        crate::dsl::Expr::Bool(b) => b.to_string(),
        crate::dsl::Expr::Null => "null".into(),
        crate::dsl::Expr::Ref { path, old } => {
            if *old {
                return "true".into();
            }
            let Some(field) = entity.fields.iter().find(|f| f.name == path[0]) else {
                return "true".into();
            };
            let col = to_camel(&to_snake_case(&field.name));
            if path.len() > 1 {
                return "true".into();
            }
            let opt = is_option_field(field);
            if is_enum(program, &field.type_name) {
                return if opt {
                    format!("(input.{col}?.name ?? '')")
                } else {
                    format!("input.{col}.name")
                };
            }
            match field.type_name.as_str() {
                "Decimal" if opt => format!("(num.tryParse(input.{col} ?? '') ?? 0)"),
                "Decimal" => format!("(num.tryParse(input.{col}) ?? 0)"),
                "Timestamp" | "Date" if opt => format!("(input.{col} ?? DateTime.now())"),
                "Int" | "Float" if opt => format!("(input.{col} ?? 0)"),
                "Bool" if opt => format!("(input.{col} ?? false)"),
                "String" | "Email" | "Text" | "Uuid" if opt => format!("(input.{col} ?? '')"),
                _ => format!("input.{col}"),
            }
        }
        crate::dsl::Expr::List(items) => {
            let parts: Vec<String> = items
                .iter()
                .map(|i| dart_expr(program, i, entity))
                .collect();
            format!("[{}]", parts.join(", "))
        }
        crate::dsl::Expr::Call(name, args) => {
            let rendered: Vec<String> =
                args.iter().map(|a| dart_expr(program, a, entity)).collect();
            match name.as_str() {
                "matches" => format!(
                    "RegExp({}).hasMatch({})",
                    rendered.get(1).cloned().unwrap_or_else(|| "''".into()),
                    rendered.first().cloned().unwrap_or_default()
                ),
                _ => "true".into(),
            }
        }
        crate::dsl::Expr::Not(inner) => format!("!({})", dart_expr(program, inner, entity)),
        crate::dsl::Expr::Bin(op, a, b) => {
            let l = dart_expr(program, a, entity);
            let r = dart_expr(program, b, entity);
            // DateTime 无 < <= > >= 运算符 → compareTo
            if (expr_is_ts(a, entity) || expr_is_ts(b, entity))
                && matches!(
                    op,
                    crate::dsl::BinOp::Lt
                        | crate::dsl::BinOp::Le
                        | crate::dsl::BinOp::Gt
                        | crate::dsl::BinOp::Ge
                        | crate::dsl::BinOp::Eq
                        | crate::dsl::BinOp::Ne
                )
            {
                let cmp = match op {
                    crate::dsl::BinOp::Lt => "<",
                    crate::dsl::BinOp::Le => "<=",
                    crate::dsl::BinOp::Gt => ">",
                    crate::dsl::BinOp::Ge => ">=",
                    crate::dsl::BinOp::Eq => "==",
                    _ => "!=",
                };
                return format!("({l}.compareTo({r}) {cmp} 0)");
            }
            let op_str = match op {
                crate::dsl::BinOp::Eq => "==",
                crate::dsl::BinOp::Ne => "!=",
                crate::dsl::BinOp::Lt => "<",
                crate::dsl::BinOp::Le => "<=",
                crate::dsl::BinOp::Gt => ">",
                crate::dsl::BinOp::Ge => ">=",
                crate::dsl::BinOp::And => "&&",
                crate::dsl::BinOp::Or => "||",
                crate::dsl::BinOp::Add => "+",
                crate::dsl::BinOp::Sub => "-",
                crate::dsl::BinOp::Mul => "*",
                crate::dsl::BinOp::Div => "/",
                crate::dsl::BinOp::In => return format!("({r}).contains({l})"),
            };
            format!("({l} {op_str} {r})")
        }
    }
}

// ── ui/format.dart ──────────────────────────────────────

fn format_dart() -> String {
    r#"// @generated by Forma (gen-flutter) — do not edit manually.

/// 列表/详情时间展示：yyyy-MM-dd HH:mm
String fmtDateTime(DateTime? t) {
  if (t == null) return '-';
  String p(int v, [int w = 2]) => v.toString().padLeft(w, '0');
  return '${p(t.year, 4)}-${p(t.month)}-${p(t.day)} ${p(t.hour)}:${p(t.minute)}';
}

/// 日期展示：yyyy-MM-dd
String fmtDate(DateTime? d) {
  if (d == null) return '-';
  String p(int v, [int w = 2]) => v.toString().padLeft(w, '0');
  return '${p(d.year, 4)}-${p(d.month)}-${p(d.day)}';
}

/// 表单 datetime 输入值：yyyy-MM-ddTHH:mm（DateTime.tryParse 可解析）
String dtInput(DateTime? t) {
  if (t == null) return '';
  final iso = t.toIso8601String();
  return iso.length >= 16 ? iso.substring(0, 16) : iso;
}
"#
    .to_string()
}

// ── 详情页 ──────────────────────────────────────────────

fn detail_value_expr(
    program: &Program,
    entity: &EntityDecl,
    path: &str,
    hint: Option<&str>,
) -> String {
    // 详情页行值：与列表页同源的类型驱动渲染（r.xxx）
    let (expr, _) = row_col_expr(program, entity, path, hint);
    expr
}

/// Detail 原语 → Material 详情页：按 section 分 Card，行 = 标签/值。
/// children（子列表）暂与 gen-web 一致：不渲染（spec 未定义其分页/布局细节）。
fn detail_page_dart(program: &Program, page: &PageDecl, cfg: &DetailCfg) -> String {
    let Some(entity) = find_entity(program, &cfg.entity) else {
        return format!(
            "// @generated by Forma (gen-flutter) — 页面 {} 引用的实体 {} 不存在。\n",
            page.name, cfg.entity
        );
    };
    let title = pick_message(&page.title, &page.name);
    let page_name = &page.name;
    let entity_name = &entity.name;
    let api_name = format!("{}Api", entity_name);

    // 每个 section → Card；每行 → label/value
    let mut needs_fmt = false;
    let mut sections_code = String::new();
    for (section, fields) in &cfg.sections {
        let mut rows = String::new();
        for (path, hint) in fields {
            let fname = path.rsplit('.').next().unwrap_or(path).to_string();
            let label = humanize(&to_snake_case(&fname));
            let expr = detail_value_expr(program, entity, path, hint.as_deref());
            if matches!(hint.as_deref(), Some("datetime"))
                || entity
                    .fields
                    .iter()
                    .find(|f| f.name == fname)
                    .map(|f| matches!(f.type_name.as_str(), "Timestamp" | "Date"))
                    .unwrap_or(false)
            {
                needs_fmt = true;
            }
            rows.push_str(&format!(
                "                        _kv('{}', {expr}),\n",
                escape_dart(&label)
            ));
        }
        sections_code.push_str(&format!(
            "              Card(\n                child: Padding(\n                  padding: const EdgeInsets.all(16),\n                  child: Column(\n                    crossAxisAlignment: CrossAxisAlignment.start,\n                    children: [\n                      Text('{}', style: Theme.of(context).textTheme.titleMedium),\n                      const SizedBox(height: 8),\n{rows}                    ],\n                  ),\n                ),\n              ),\n              const SizedBox(height: 12),\n",
            escape_dart(&humanize(section)),
            rows = rows
        ));
    }

    let fmt_import = if needs_fmt {
        "import '../ui/format.dart';\n"
    } else {
        ""
    };

    let template = r#"// @generated by Forma (gen-flutter) — do not edit manually.
import 'package:flutter/material.dart';

import '../api/api.dart';
import '../models/models.dart';
%%FMT_IMPORT%%
/// %%TITLE%%
class %%PAGE%% extends StatefulWidget {
  final String? id;
  const %%PAGE%%({super.key, this.id});

  @override
  State<%%PAGE%%> createState() => _%%PAGE%%State();
}

class _%%PAGE%%State extends State<%%PAGE%%> {
  final Api _api = Api();
  %%ENTITY%%? _row;
  bool _loading = true;
  String? _error;

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    final id = widget.id;
    if (id == null || id == 'new') {
      setState(() => _loading = false);
      return;
    }
    try {
      final row = await %%API%%(_api).get(id);
      if (!mounted) return;
      setState(() {
        _row = row;
        _loading = false;
      });
    } on ApiException catch (e) {
      if (!mounted) return;
      setState(() {
        _error = e.message;
        _loading = false;
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final r = _row;
    return Scaffold(
      appBar: AppBar(title: const Text('%%TITLE%%')),
      body: _loading
          ? const Center(child: CircularProgressIndicator())
          : _error != null
              ? Center(child: Text(_error!))
              : r == null
                  ? const Center(child: Text('未选择记录'))
                  : ListView(
                      padding: const EdgeInsets.all(12),
                      children: [
%%SECTIONS%%                      ],
                    ),
    );
  }

  Widget _kv(String label, String value) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 4),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          SizedBox(
            width: 120,
            child: Text(label,
                style: const TextStyle(color: Colors.black54)),
          ),
          Expanded(child: Text(value)),
        ],
      ),
    );
  }
}
"#;

    template
        .replace("%%FMT_IMPORT%%", fmt_import)
        .replace("%%TITLE%%", &escape_dart(&title))
        .replace("%%PAGE%%", page_name)
        .replace("%%ENTITY%%", entity_name)
        .replace("%%API%%", &api_name)
        .replace("%%SECTIONS%%", &sections_code)
}

// ── 列表页 ──────────────────────────────────────────────

struct ColRender {
    label: String,
    expr: String,
}

fn row_col_expr(
    program: &Program,
    entity: &EntityDecl,
    path: &str,
    _hint: Option<&str>,
) -> (String, bool) {
    let fname = path.rsplit('.').next().unwrap_or(path).to_string();
    let col = to_snake_case(&fname);
    let field = entity.fields.iter().find(|f| f.name == fname);
    if field
        .map(|f| is_relation(program, &f.type_name))
        .unwrap_or(false)
    {
        let cam = to_camel(&col);
        return (format!("r.{cam}Name ?? r.{cam}Id"), false);
    }
    let t = field.map(|f| f.type_name.as_str()).unwrap_or("String");
    let opt = field.map(|f| f.optional).unwrap_or(false);
    let cam = to_camel(&col);
    let expr = match t {
        "Timestamp" => format!("fmtDateTime(r.{cam})"),
        "Date" => format!("fmtDate(r.{cam})"),
        "Int" | "Float" if opt => format!("r.{cam}?.toString() ?? '-'"),
        "Int" | "Float" => format!("r.{cam}.toString()"),
        "Bool" => format!("r.{cam} ? '是' : '否'"),
        other if is_enum(program, other) => {
            if opt {
                format!("r.{cam}?.label ?? '-'")
            } else {
                format!("r.{cam}.label")
            }
        }
        _ if opt => format!("r.{cam} ?? '-'"),
        _ => format!("r.{cam}"),
    };
    (expr, matches!(t, "Timestamp" | "Date"))
}

enum FilterKind {
    EnumField(String),
    Relation(String),
}

struct FilterGen {
    pname: String,
    pascal: String,
    label: String,
    kind: FilterKind,
}

fn list_page_dart(program: &Program, page: &PageDecl, cfg: &TableCfg) -> String {
    let Some(api) = api_of_by_name(program, &cfg.query_api) else {
        return format!(
            "// @generated by Forma (gen-flutter) — do not edit manually.\n// 页面 {} 引用的 API {} 不存在。\n",
            page.name, cfg.query_api
        );
    };
    let Some(entity) = find_entity(program, &api.entity) else {
        return "// @generated by Forma (gen-flutter) — unknown entity.\n".into();
    };
    let title = pick_message(&page.title, &page.name);
    let page_name = &page.name;
    let row_type = format!("{}ListRow", entity.name);
    let api_name = format!("{}Api", entity.name);
    let params_name = format!("{}ListParams", entity.name);

    // ── 列渲染 ──
    let mut cols: Vec<ColRender> = Vec::new();
    let mut needs_fmt = false;
    for c in &cfg.columns {
        let fname = c.path.rsplit('.').next().unwrap_or(&c.path).to_string();
        let (expr, nf) = row_col_expr(program, entity, &c.path, c.hint.as_deref());
        needs_fmt |= nf;
        let _ = nf;
        cols.push(ColRender {
            label: humanize(&to_snake_case(&fname)),
            expr,
        });
    }
    if cols.is_empty() {
        cols.push(ColRender {
            label: "Id".into(),
            expr: "r.id".into(),
        });
    }
    let title_expr = cols[0].expr.clone();
    let lines = cols[1..]
        .iter()
        .map(|c| format!("    '{}：${{{}}}',", escape_dart(&c.label), c.expr))
        .collect::<Vec<_>>()
        .join("\n");

    // ── 过滤器 ──
    let mut filters: Vec<FilterGen> = Vec::new();
    for f in &cfg.filters {
        let fname = f.path.rsplit('.').next().unwrap_or(&f.path).to_string();
        let Some(field) = entity.fields.iter().find(|x| x.name == fname) else {
            continue;
        };
        let pname = to_snake_case(&fname);
        let pascal = humanize(&pname).replace(' ', "");
        let label = humanize(&pname);
        if is_enum(program, &field.type_name) {
            filters.push(FilterGen {
                pname,
                pascal,
                label,
                kind: FilterKind::EnumField(field.type_name.clone()),
            });
        } else if is_relation(program, &field.type_name) {
            filters.push(FilterGen {
                pname,
                pascal,
                label,
                kind: FilterKind::Relation(field.type_name.clone()),
            });
        }
    }

    // 关系过滤器的选项源（去重）
    let mut sources: Vec<String> = Vec::new();
    for f in &filters {
        if let FilterKind::Relation(src) = &f.kind {
            if !sources.contains(src) {
                sources.push(src.clone());
            }
        }
    }

    // ── 动作 ──
    let mut fab = String::new();
    let mut row_nav = String::new();
    let mut trailing_edit = String::new();
    let mut has_delete = false;
    if let Some(actions) = &cfg.actions {
        if let Some(create) = &actions.create {
            if let Some(target) = program.pages.iter().find(|p| p.name == *create) {
                let route = target.route.replace("{id}", "new");
                fab = format!(
                    "      floatingActionButton: FloatingActionButton.extended(\n        onPressed: () => Navigator.pushNamed(context, '{}'),\n        icon: const Icon(Icons.add),\n        label: const Text('新建'),\n      ),\n",
                    escape_dart(&route)
                );
            }
        }
        let mut edit_route: Option<String> = None;
        for (action, target) in &actions.row {
            if action == "edit" {
                if let Some(tp) = program.pages.iter().find(|p| p.name == *target) {
                    edit_route = Some(tp.route.clone());
                }
            }
            if action == "delete" && target.ends_with(".delete") {
                has_delete = api.operations.iter().any(|o| o == "delete");
            }
        }
        if let Some(route) = edit_route {
            let dart_route = route.replace("{id}", "${r.id}");
            row_nav = format!(
                "                        onTap: () => Navigator.pushNamed(context, '{}'),\n",
                dart_route
            );
            trailing_edit = format!(
                "                          IconButton(\n                            tooltip: '编辑',\n                            icon: const Icon(Icons.edit_outlined),\n                            onPressed: () => Navigator.pushNamed(context, '{}'),\n                          ),\n",
                dart_route
            );
        }
    }
    let trailing_delete = if has_delete {
        "                          IconButton(\n                            tooltip: '删除',\n                            icon: const Icon(Icons.delete_outline),\n                            onPressed: () => _delete(r.id),\n                          ),\n"
    } else {
        ""
    };
    let trailing = if trailing_edit.is_empty() && trailing_delete.is_empty() {
        String::new()
    } else {
        format!(
            "                        trailing: Row(\n                          mainAxisSize: MainAxisSize.min,\n                          children: [\n{trailing_edit}{trailing_delete}                          ],\n                        ),\n"
        )
    };

    // ── 状态字段 / 选项加载 / 查询参数 ──
    let has_search = !cfg.search.is_empty();
    let mut state_fields = String::new();
    if has_search {
        state_fields.push_str(
            "  final TextEditingController _searchCtrl = TextEditingController();\n  String? _q;\n",
        );
    }
    for f in &filters {
        state_fields.push_str(&format!("  String? _f{};\n", f.pascal));
    }
    for src in &sources {
        let src_snake = to_snake_case(src);
        state_fields.push_str(&format!(
            "  List<{src}ListRow> _{src_snake}Options = const [];\n"
        ));
    }

    let mut query_args = vec!["pageSize: 50".to_string()];
    if has_search {
        query_args.push("q: _q".into());
    }
    for f in &filters {
        query_args.push(format!("{}: _f{}", to_camel(&f.pname), f.pascal));
    }

    let mut load_options = String::new();
    if !sources.is_empty() {
        let mut fetches = String::new();
        let mut assigns = String::new();
        for src in &sources {
            let src_snake = to_snake_case(src);
            fetches.push_str(&format!(
                "      final {src_snake}Options = await {src}Api(_api).list({src}ListParams(pageSize: 200));\n"
            ));
            assigns.push_str(&format!(
                "      _{src_snake}Options = {src_snake}Options;\n"
            ));
        }
        load_options = format!(
            "  Future<void> _loadOptions() async {{\n    try {{\n{fetches}      if (!mounted) return;\n      setState(() {{\n{assigns}      }});\n    }} on ApiException {{\n      // 选项加载失败不阻塞列表\n    }}\n  }}\n\n"
        );
    }

    let init_options: &str = if sources.is_empty() {
        ""
    } else {
        "    _loadOptions();\n"
    };

    let search_block = if has_search {
        let hint = cfg
            .search
            .iter()
            .map(|s| humanize(&to_snake_case(s)))
            .collect::<Vec<_>>()
            .join("、");
        format!(
            "          Padding(\n            padding: const EdgeInsets.fromLTRB(12, 12, 12, 0),\n            child: TextField(\n              controller: _searchCtrl,\n              decoration: const InputDecoration(\n                prefixIcon: Icon(Icons.search),\n                hintText: '搜索 {hint}',\n                border: OutlineInputBorder(),\n                isDense: true,\n              ),\n              onSubmitted: (v) {{\n                setState(() => _q = v.trim().isEmpty ? null : v.trim());\n                _reload();\n              }},\n            ),\n          ),\n",
            hint = escape_dart(&hint)
        )
    } else {
        String::new()
    };

    let mut filter_blocks = String::new();
    for f in &filters {
        let items = match &f.kind {
            FilterKind::EnumField(enum_name) => {
                let Some(e) = program.enums.iter().find(|e| e.name == *enum_name) else {
                    continue;
                };
                let rows = e
                    .variants
                    .iter()
                    .map(|v| {
                        let label = pick_message(&v.labels, &v.name);
                        format!(
                            "                DropdownMenuItem(value: '{}', child: Text('{}')),\n",
                            v.name,
                            escape_dart(&label)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("");
                format!(
                    "              items: const [\n                DropdownMenuItem(value: '', child: Text('全部')),\n{items}              ],\n",
                    items = rows
                )
            }
            FilterKind::Relation(src) => {
                let src_snake = to_snake_case(src);
                let display = find_entity(program, src)
                    .map(display_field_of)
                    .map(|d| to_camel(&d))
                    .unwrap_or_else(|| "id".into());
                format!(
                    "              items: [\n                const DropdownMenuItem(value: '', child: Text('全部')),\n                for (final o in _{src_snake}Options)\n                  DropdownMenuItem(value: o.id, child: Text(o.{display})),\n              ],\n"
                )
            }
        };
        filter_blocks.push_str(&format!(
            "          Padding(\n            padding: const EdgeInsets.fromLTRB(12, 8, 12, 0),\n            child: DropdownButtonFormField<String>(\n              initialValue: '',\n              decoration: const InputDecoration(labelText: '{label}', border: OutlineInputBorder(), isDense: true),\n{items}              onChanged: (v) {{\n                setState(() => _f{pascal} = (v == null || v.isEmpty) ? null : v);\n                _reload();\n              }},\n            ),\n          ),\n",
            label = escape_dart(&f.label),
            items = items,
            pascal = f.pascal,
        ));
    }

    let delete_fn = if has_delete {
        format!(
            "  Future<void> _delete(String id) async {{\n    final ok = await showDialog<bool>(\n      context: context,\n      builder: (ctx) => AlertDialog(\n        title: const Text('删除确认'),\n        content: const Text('确认删除该条记录？此操作不可恢复。'),\n        actions: [\n          TextButton(onPressed: () => Navigator.pop(ctx, false), child: const Text('取消')),\n          FilledButton(onPressed: () => Navigator.pop(ctx, true), child: const Text('删除')),\n        ],\n      ),\n    );\n    if (ok != true) return;\n    try {{\n      await {api_name}(_api).remove(id);\n      if (!mounted) return;\n      _reload();\n    }} on ApiException catch (e) {{\n      if (!mounted) return;\n      ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(e.message)));\n    }}\n  }}\n\n"
        )
    } else {
        String::new()
    };

    let dispose_fn = if has_search {
        "  @override\n  void dispose() {\n    _searchCtrl.dispose();\n    super.dispose();\n  }\n\n"
    } else {
        ""
    };

    let fmt_import = if needs_fmt {
        "import '../ui/format.dart';\n"
    } else {
        ""
    };

    let template = r#"// @generated by Forma (gen-flutter) — do not edit manually.
import 'package:flutter/material.dart';

import '../api/api.dart';
import '../models/models.dart';
%%FMT_IMPORT%%
/// %%TITLE%%
class %%PAGE%% extends StatefulWidget {
  const %%PAGE%%({super.key});

  @override
  State<%%PAGE%%> createState() => _%%PAGE%%State();
}

class _%%PAGE%%State extends State<%%PAGE%%> {
  final Api _api = Api();
%%STATE_FIELDS%%  late Future<List<%%ROW%%>> _future;

  @override
  void initState() {
    super.initState();
    _reload();
%%INIT_OPTIONS%%  }

  void _reload() {
    setState(() {
      _future = %%API%%(_api).list(
        %%PARAMS%%(%%QUERY_ARGS%%),
      );
    });
  }

%%LOAD_OPTIONS%%%%DELETE_FN%%%%DISPOSE_FN%%  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('%%TITLE%%')),
%%FAB%%      body: Column(
        children: [
%%SEARCH%%%%FILTERS%%          Expanded(
            child: FutureBuilder<List<%%ROW%%>>(
              future: _future,
              builder: (context, snap) {
                if (snap.connectionState != ConnectionState.done) {
                  return const Center(child: CircularProgressIndicator());
                }
                if (snap.hasError) {
                  return Center(child: Text('加载失败：${snap.error}'));
                }
                final rows = snap.data ?? const <%%ROW%%>[];
                if (rows.isEmpty) {
                  return const Center(child: Text('暂无数据'));
                }
                return ListView.separated(
                  padding: const EdgeInsets.all(12),
                  itemCount: rows.length,
                  separatorBuilder: (_, __) => const SizedBox(height: 8),
                  itemBuilder: (context, index) {
                    final r = rows[index];
                    return Card(
                      child: ListTile(
                        title: Text(%%TITLE_EXPR%%),
                        subtitle: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            for (final line in _detailLines(r))
                              Padding(
                                padding: const EdgeInsets.only(top: 2),
                                child: Text(line),
                              ),
                          ],
                        ),
%%TRAILING%%%%ON_TAP%%                      ),
                    );
                  },
                );
              },
            ),
          ),
        ],
      ),
    );
  }

  List<String> _detailLines(%%ROW%% r) => [
%%LINES%%
  ];
}
"#;

    template
        .replace("%%TITLE%%", &escape_dart(&title))
        .replace("%%PAGE%%", page_name)
        .replace("%%ROW%%", &row_type)
        .replace("%%API%%", &api_name)
        .replace("%%PARAMS%%", &params_name)
        .replace("%%QUERY_ARGS%%", &query_args.join(", "))
        .replace("%%STATE_FIELDS%%", &state_fields)
        .replace("%%INIT_OPTIONS%%", init_options)
        .replace("%%LOAD_OPTIONS%%", &load_options)
        .replace("%%DELETE_FN%%", &delete_fn)
        .replace("%%DISPOSE_FN%%", dispose_fn)
        .replace("%%FAB%%", &fab)
        .replace("%%SEARCH%%", &search_block)
        .replace("%%FILTERS%%", &filter_blocks)
        .replace("%%FMT_IMPORT%%", fmt_import)
        .replace("%%TITLE_EXPR%%", &title_expr)
        .replace("%%TRAILING%%", &trailing)
        .replace("%%ON_TAP%%", &row_nav)
        .replace("%%LINES%%", &lines)
}

// ── 编辑页 ──────────────────────────────────────────────

#[derive(Clone)]
enum FieldKind {
    Text,
    Multiline,
    Email,
    Int,
    Float,
    Decimal,
    Bool,
    Dt,
    Relation(String),
    EnumField(String),
}

struct FieldGen {
    col: String,        // Dart 标识符（camel）
    key: String,        // 实体 JSON 键名（关系 = {rel}_id）
    input_name: String, // 输入体键名（关系 = 裸关系名，camel）
    label: String,
    kind: FieldKind,
    required: bool,
}

fn edit_page_dart(program: &Program, page: &PageDecl, cfg: &FormCfg) -> String {
    let Some(entity) = find_entity(program, &cfg.entity) else {
        return format!(
            "// @generated by Forma (gen-flutter) — 页面 {} 引用的实体 {} 不存在。\n",
            page.name, cfg.entity
        );
    };
    let title = pick_message(&page.title, &page.name);
    let page_name = &page.name;
    let entity_name = &entity.name;
    let api_name = format!("{}Api", entity_name);
    let entity_label = humanize(&to_snake_case(entity_name));

    let has_create = cfg.submits.iter().any(|s| s.is_create);
    let has_update = cfg.submits.iter().any(|s| !s.is_create);
    let submit_create = cfg.submits.iter().find(|s| s.is_create);
    let submit_update = cfg.submits.iter().find(|s| !s.is_create);
    let redirect = if has_update {
        submit_update
            .map(|s| s.redirect.clone())
            .unwrap_or_default()
    } else {
        submit_create
            .map(|s| s.redirect.clone())
            .unwrap_or_default()
    };

    // rules 索引
    let mut rules_by_field: std::collections::HashMap<String, Vec<&crate::dsl::FieldConstraint>> =
        Default::default();
    for rules in &program.rules {
        for fr in &rules.field_rules {
            if fr.entity == *entity_name {
                rules_by_field
                    .entry(fr.field.clone())
                    .or_default()
                    .extend(fr.constraints.iter());
            }
        }
    }

    // ── 字段分类 ──
    let mut fields: Vec<FieldGen> = Vec::new();
    for f in &cfg.fields {
        let fname_dsl = f.path.rsplit('.').next().unwrap_or(&f.path).to_string();
        let Some(field) = entity.fields.iter().find(|x| x.name == fname_dsl) else {
            continue;
        };
        let key = json_key(program, field); // 实体 JSON 键（关系带 _id）
        let col = to_camel(&key); // Dart 标识符
        let input_name = to_camel(&to_snake_case(&fname_dsl)); // 输入体键（关系裸名）
        let label = humanize(&to_snake_case(&fname_dsl));
        let required = f.required || !is_option_field(field);
        let kind = if is_enum(program, f.from.as_deref().unwrap_or("")) {
            FieldKind::EnumField(field.type_name.clone())
        } else if let Some(src) = f
            .from
            .as_deref()
            .filter(|s| find_entity(program, s).is_some())
        {
            FieldKind::Relation(src.to_string())
        } else if is_relation(program, &field.type_name) {
            FieldKind::Relation(field.type_name.clone())
        } else if is_enum(program, &field.type_name) {
            FieldKind::EnumField(field.type_name.clone())
        } else if f.widget == "Textarea" {
            FieldKind::Multiline
        } else if matches!(field.type_name.as_str(), "Timestamp" | "Date")
            || f.widget.contains("datetime")
        {
            FieldKind::Dt
        } else if field.type_name == "Int" {
            FieldKind::Int
        } else if field.type_name == "Float" {
            FieldKind::Float
        } else if field.type_name == "Decimal" {
            FieldKind::Decimal
        } else if field.type_name == "Bool" {
            FieldKind::Bool
        } else if f.widget.contains("email") || field.type_name == "Email" {
            FieldKind::Email
        } else {
            FieldKind::Text
        };
        fields.push(FieldGen {
            col,
            key,
            input_name,
            label,
            kind,
            required,
        });
    }

    // ── 表单未覆盖的必填输入（如 Candidate.election）：补空参交后端拒绝 ──
    let mut missing_args: Vec<String> = Vec::new();
    for field in &entity.fields {
        if has_attr(field, "id") || is_option_field(field) {
            continue;
        }
        let iname = to_camel(&to_snake_case(&field.name));
        if fields.iter().any(|g| g.input_name == iname) {
            continue;
        }
        let arg = if is_relation(program, &field.type_name)
            || matches!(
                field.type_name.as_str(),
                "String" | "Email" | "Text" | "Decimal" | "Uuid"
            ) {
            "''".to_string()
        } else if field.type_name == "Int" || field.type_name == "Float" {
            "0".to_string()
        } else if field.type_name == "Bool" {
            "false".to_string()
        } else if matches!(field.type_name.as_str(), "Timestamp" | "Date") {
            "DateTime.now()".to_string()
        } else if is_enum(program, &field.type_name) {
            let Some(e) = program.enums.iter().find(|e| e.name == field.type_name) else {
                continue;
            };
            format!("{}.{}", e.name, e.variants[0].name)
        } else {
            "''".to_string()
        };
        missing_args.push(format!(
            "      {iname}: {arg}, // 表单未提供，交后端校验拒绝\n"
        ));
    }

    // ── readonly 时间字段回传状态（check 规则引用，如 createdAt） ──
    let mut passthrough: Vec<(String, String)> = Vec::new(); // (camel col, json key)
    for field in &entity.fields {
        if has_attr(field, "id") || !has_attr(field, "readonly") {
            continue;
        }
        if !matches!(field.type_name.as_str(), "Timestamp" | "Date") {
            continue;
        }
        let key = json_key(program, field);
        let col = to_camel(&key);
        if fields.iter().any(|g| g.col == col) {
            continue;
        }
        passthrough.push((col, key));
    }

    // ── 状态字段 ──
    let mut state_fields = String::new();
    let mut dispose_lines = String::new();
    for g in &fields {
        match &g.kind {
            FieldKind::Relation(src) => {
                let src_camel = to_camel(&to_snake_case(src));
                state_fields.push_str(&format!("  String? _{};\n", g.col));
                state_fields.push_str(&format!(
                    "  List<{src}ListRow> _{src_camel}Options = const [];\n"
                ));
            }
            FieldKind::EnumField(e) => {
                state_fields.push_str(&format!("  {}? _{};\n", e, g.col));
            }
            FieldKind::Bool => {
                state_fields.push_str(&format!("  bool? _{};\n", g.col));
            }
            _ => {
                state_fields.push_str(&format!(
                    "  final TextEditingController _{}Ctrl = TextEditingController();\n",
                    g.col
                ));
                dispose_lines.push_str(&format!("    _{}Ctrl.dispose();\n", g.col));
            }
        }
    }
    for (col, _) in &passthrough {
        state_fields.push_str(&format!("  DateTime? _{col};\n"));
    }

    // ── 选项加载 ──
    let mut sources: Vec<String> = Vec::new();
    for g in &fields {
        if let FieldKind::Relation(src) = &g.kind {
            if !sources.contains(src) {
                sources.push(src.clone());
            }
        }
    }
    let mut load_options = String::new();
    if !sources.is_empty() {
        let mut fetches = String::new();
        let mut assigns = String::new();
        for src in &sources {
            let src_camel = to_camel(&to_snake_case(src));
            fetches.push_str(&format!(
                "      final {src_camel}Options = await {src}Api(_api).list({src}ListParams(pageSize: 200));\n"
            ));
            assigns.push_str(&format!(
                "      _{src_camel}Options = {src_camel}Options;\n"
            ));
        }
        load_options = format!(
            "  Future<void> _loadOptions() async {{\n    try {{\n{fetches}      if (!mounted) return;\n      setState(() {{\n{assigns}      }});\n    }} on ApiException {{\n      // 选项加载失败不阻塞表单\n    }}\n  }}\n\n"
        );
    }

    // ── 加载已有行（update 模式）：非空字段不写 dead ?? ──
    let mut load_lines = String::new();
    if has_update {
        for g in &fields {
            let key = &to_camel(&g.key);
            let col = &g.col;
            let opt_field = entity
                .fields
                .iter()
                .find(|f| to_camel(&json_key(program, f)) == *col)
                .map(|f| f.optional)
                .unwrap_or(false);
            match &g.kind {
                FieldKind::Relation(_) | FieldKind::EnumField(_) | FieldKind::Bool => {
                    load_lines.push_str(&format!("        _{col} = row.{key};\n"));
                }
                FieldKind::Dt => {
                    load_lines
                        .push_str(&format!("        _{col}Ctrl.text = dtInput(row.{key});\n"));
                }
                FieldKind::Int | FieldKind::Float => {
                    if opt_field {
                        load_lines.push_str(&format!(
                            "        _{col}Ctrl.text = row.{key}?.toString() ?? '';\n"
                        ));
                    } else {
                        load_lines.push_str(&format!(
                            "        _{col}Ctrl.text = row.{key}.toString();\n"
                        ));
                    }
                }
                FieldKind::Decimal => {
                    if opt_field {
                        load_lines
                            .push_str(&format!("        _{col}Ctrl.text = row.{key} ?? '';\n"));
                    } else {
                        load_lines.push_str(&format!("        _{col}Ctrl.text = row.{key};\n"));
                    }
                }
                _ => {
                    if opt_field {
                        load_lines
                            .push_str(&format!("        _{col}Ctrl.text = row.{key} ?? '';\n"));
                    } else {
                        load_lines.push_str(&format!("        _{col}Ctrl.text = row.{key};\n"));
                    }
                }
            }
        }
        for (col, key) in &passthrough {
            load_lines.push_str(&format!("        _{col} = row.{};\n", to_camel(key)));
        }
    }
    let load_fn = if has_update {
        format!(
            "  Future<void> _load() async {{\n    setState(() => _loading = true);\n    try {{\n      final row = await {api_name}(_api).get(widget.id!);\n      if (!mounted) return;\n      setState(() {{\n{load_lines}        _loading = false;\n      }});\n    }} on ApiException catch (e) {{\n      if (!mounted) return;\n      setState(() {{\n        _error = e.message;\n        _loading = false;\n      }});\n    }}\n  }}\n\n"
        )
    } else {
        String::new()
    };

    // ── 输入体 ──
    let mut input_args: Vec<String> = Vec::new();
    for g in &fields {
        let iname = &g.input_name;
        let col = &g.col;
        let arg = match &g.kind {
            FieldKind::Relation(_) => {
                if g.required {
                    format!("{iname}: _{col} ?? ''")
                } else {
                    format!("{iname}: _{col}")
                }
            }
            FieldKind::EnumField(_) => {
                if g.required {
                    format!("{iname}: _{col}!")
                } else {
                    format!("{iname}: _{col}")
                }
            }
            FieldKind::Bool => {
                if g.required {
                    format!("{iname}: _{col} ?? false")
                } else {
                    format!("{iname}: _{col}")
                }
            }
            FieldKind::Dt => {
                if g.required {
                    format!("{iname}: (DateTime.tryParse(_{col}Ctrl.text.trim()) ?? DateTime.now()).toUtc()")
                } else {
                    format!("{iname}: _parseDt(_{col}Ctrl.text)")
                }
            }
            FieldKind::Int => {
                if g.required {
                    format!("{iname}: int.parse(_{col}Ctrl.text)")
                } else {
                    format!(
                        "{iname}: _{col}Ctrl.text.isEmpty ? null : int.tryParse(_{col}Ctrl.text)"
                    )
                }
            }
            FieldKind::Float => {
                if g.required {
                    format!("{iname}: double.parse(_{col}Ctrl.text)")
                } else {
                    format!("{iname}: _{col}Ctrl.text.isEmpty ? null : double.tryParse(_{col}Ctrl.text)")
                }
            }
            FieldKind::Decimal => {
                if g.required {
                    format!("{iname}: _{col}Ctrl.text")
                } else {
                    format!("{iname}: _{col}Ctrl.text.isEmpty ? null : _{col}Ctrl.text")
                }
            }
            _ => {
                if g.required {
                    format!("{iname}: _{col}Ctrl.text")
                } else {
                    format!("{iname}: _{col}Ctrl.text.isEmpty ? null : _{col}Ctrl.text")
                }
            }
        };
        input_args.push(format!("      {arg},\n"));
    }
    for (col, key) in &passthrough {
        input_args.push(format!(
            "      {}: _{},\n",
            to_camel(&to_snake_case(key)),
            col
        ));
    }
    for arg in &missing_args {
        input_args.push(arg.clone());
    }

    // ── 校验 / 提交 ──
    let validate_block = if cfg.validation.is_empty() {
        String::new()
    } else {
        let call = if has_create && has_update {
            format!(
                "final errs = _isCreate ? validateCreate{0}(input) : validateUpdate{0}(input);",
                entity_name
            )
        } else if has_create {
            format!("final errs = validateCreate{0}(input);", entity_name)
        } else {
            format!("final errs = validateUpdate{0}(input);", entity_name)
        };
        format!(
            "    {call}\n    if (errs.isNotEmpty) {{\n      setState(() {{\n        _error = errs.map((e) => '${{e.field}}: ${{e.message}}').join('；');\n      }});\n      return;\n    }}\n"
        )
    };

    let submit_block = if has_create && has_update {
        format!(
            "      if (_isCreate) {{\n        await {api_name}(_api).create(input);\n      }} else {{\n        await {api_name}(_api).update(widget.id!, input);\n      }}\n"
        )
    } else if has_create {
        format!("      await {api_name}(_api).create(input);\n")
    } else {
        format!("      await {api_name}(_api).update(widget.id!, input);\n")
    };

    let is_create_expr = if has_create && has_update {
        "widget.id == null || widget.id == 'new'"
    } else if has_create {
        "true"
    } else {
        "false"
    };

    let init_body = format!(
        "{}{}",
        if has_update {
            "    if (!_isCreate) _load();\n"
        } else {
            ""
        },
        if sources.is_empty() {
            ""
        } else {
            "    _loadOptions();\n"
        }
    );

    // ── 字段控件 ──
    let mut field_widgets = String::new();
    for g in &fields {
        let col = &g.col;
        let label = escape_dart(&g.label);
        let required_msg = rules_by_field
            .get(&g.key)
            .and_then(|cs| {
                cs.iter().find_map(|c| match c {
                    crate::dsl::FieldConstraint::Required(msg) => {
                        Some(pick_message(msg, "required"))
                    }
                    _ => None,
                })
            })
            .unwrap_or_else(|| format!("{} 必填", g.label));
        let (mins, maxs) = {
            let mut mins: Vec<(f64, String)> = Vec::new();
            let mut maxs: Vec<(f64, String)> = Vec::new();
            if let Some(cs) = rules_by_field.get(&g.key) {
                for c in cs {
                    match c {
                        crate::dsl::FieldConstraint::Min(v, msg) => {
                            mins.push((*v, pick_message(msg, "too small")))
                        }
                        crate::dsl::FieldConstraint::Max(v, msg) => {
                            maxs.push((*v, pick_message(msg, "too large")))
                        }
                        _ => {}
                    }
                }
            }
            if let Some(f) = entity
                .fields
                .iter()
                .find(|f| to_camel(&json_key(program, f)) == *col)
            {
                for attr in &f.attrs {
                    if matches!(attr.name.as_str(), "min" | "max") {
                        if let Some(v) = attr.args.first().and_then(|a| a.parse::<f64>().ok()) {
                            let msg = format!(
                                "{} must be {} {}",
                                col,
                                if attr.name == "min" { ">=" } else { "<=" },
                                num_literal(v)
                            );
                            if attr.name == "min" {
                                mins.push((v, msg));
                            } else {
                                maxs.push((v, msg));
                            }
                        }
                    }
                }
            }
            (mins, maxs)
        };

        let widget = match &g.kind {
            FieldKind::Relation(src) => {
                let src_camel = to_camel(&to_snake_case(src));
                let display = find_entity(program, src)
                    .map(display_field_of)
                    .map(|d| to_camel(&d))
                    .unwrap_or_else(|| "id".into());
                let validator = if g.required {
                    format!(
                        "                    validator: (v) => (v == null || v.isEmpty) ? '{}' : null,\n",
                        escape_dart(&format!("请选择 {}", g.label))
                    )
                } else {
                    String::new()
                };
                format!(
                    "                  DropdownButtonFormField<String>(\n                    initialValue: _{col} ?? '',\n                    decoration: const InputDecoration(labelText: '{label}'),\n                    items: [\n                      const DropdownMenuItem(value: '', child: Text('— 请选择 —')),\n                      for (final o in _{src_camel}Options)\n                        DropdownMenuItem(value: o.id, child: Text(o.{display})),\n                    ],\n                    onChanged: (v) => setState(() => _{col} = (v == null || v.isEmpty) ? null : v),\n{validator}                  ),\n"
                )
            }
            FieldKind::EnumField(enum_name) => {
                let Some(e) = program.enums.iter().find(|e| e.name == *enum_name) else {
                    continue;
                };
                let items = e
                    .variants
                    .iter()
                    .map(|v| {
                        let vl = pick_message(&v.labels, &v.name);
                        format!(
                            "                      DropdownMenuItem(value: '{}', child: Text('{}')),\n",
                            v.name,
                            escape_dart(&vl)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("");
                let validator = if g.required {
                    format!(
                        "                    validator: (v) => (v == null || v.isEmpty) ? '{}' : null,\n",
                        escape_dart(&format!("请选择 {}", g.label))
                    )
                } else {
                    String::new()
                };
                let parser = to_camel(&to_snake_case(enum_name));
                format!(
                    "                  DropdownButtonFormField<String>(\n                    initialValue: _{col}?.name ?? '',\n                    decoration: const InputDecoration(labelText: '{label}'),\n                    items: const [\n                      DropdownMenuItem(value: '', child: Text('— 请选择 —')),\n{items}                    ],\n                    onChanged: (v) => setState(() => _{col} = (v == null || v.isEmpty) ? null : {parser}FromName(v)),\n{validator}                  ),\n"
                )
            }
            FieldKind::Bool => {
                format!(
                    "                  CheckboxListTile(\n                    value: _{col},\n                    onChanged: (v) => setState(() => _{col} = v),\n                    title: const Text('{label}'),\n                    controlAffinity: ListTileControlAffinity.leading,\n                    contentPadding: EdgeInsets.zero,\n                  ),\n"
                )
            }
            FieldKind::Dt => {
                let empty_msg = if g.required {
                    format!("'{}'", escape_dart(&format!("请输入 {}", g.label)))
                } else {
                    "null".to_string()
                };
                format!(
                    "                  TextFormField(\n                    controller: _{col}Ctrl,\n                    decoration: const InputDecoration(labelText: '{label}', helperText: '格式：2025-01-01T09:00'),\n                    validator: (v) {{\n                      final t = v?.trim() ?? '';\n                      if (t.isEmpty) return {empty_msg};\n                      if (DateTime.tryParse(t) == null) return '日期格式无效';\n                      return null;\n                    }},\n                  ),\n"
                )
            }
            FieldKind::Int | FieldKind::Float => {
                let parser = if matches!(g.kind, FieldKind::Int) {
                    "int.tryParse(v)"
                } else {
                    "double.tryParse(v)"
                };
                let empty_msg = if g.required {
                    format!("'{}'", escape_dart(&format!("请输入 {}", g.label)))
                } else {
                    "null".to_string()
                };
                let mut checks = String::new();
                for (v, msg) in &mins {
                    checks.push_str(&format!(
                        "                      if (n < {}) return '{}';\n",
                        num_literal(*v),
                        escape_dart(msg)
                    ));
                }
                for (v, msg) in &maxs {
                    checks.push_str(&format!(
                        "                      if (n > {}) return '{}';\n",
                        num_literal(*v),
                        escape_dart(msg)
                    ));
                }
                format!(
                    "                  TextFormField(\n                    controller: _{col}Ctrl,\n                    keyboardType: TextInputType.number,\n                    decoration: const InputDecoration(labelText: '{label}'),\n                    validator: (v) {{\n                      if (v == null || v.isEmpty) return {empty_msg};\n                      final n = {parser};\n                      if (n == null) return '请输入数字';\n{checks}                      return null;\n                    }},\n                  ),\n"
                )
            }
            FieldKind::Multiline => {
                let validator = if g.required {
                    format!(
                        "                    validator: (v) => (v == null || v.trim().isEmpty) ? '{}' : null,\n",
                        escape_dart(&required_msg)
                    )
                } else {
                    String::new()
                };
                format!(
                    "                  TextFormField(\n                    controller: _{col}Ctrl,\n                    minLines: 3,\n                    maxLines: 4,\n                    decoration: const InputDecoration(labelText: '{label}'),\n{validator}                  ),\n"
                )
            }
            FieldKind::Email => {
                let validator = if g.required {
                    format!(
                        "                    validator: (v) => (v == null || v.trim().isEmpty) ? '{}' : null,\n",
                        escape_dart(&required_msg)
                    )
                } else {
                    String::new()
                };
                format!(
                    "                  TextFormField(\n                    controller: _{col}Ctrl,\n                    keyboardType: TextInputType.emailAddress,\n                    decoration: const InputDecoration(labelText: '{label}'),\n{validator}                  ),\n"
                )
            }
            _ => {
                let validator = if g.required {
                    format!(
                        "                    validator: (v) => (v == null || v.trim().isEmpty) ? '{}' : null,\n",
                        escape_dart(&required_msg)
                    )
                } else {
                    String::new()
                };
                format!(
                    "                  TextFormField(\n                    controller: _{col}Ctrl,\n                    decoration: const InputDecoration(labelText: '{label}'),\n{validator}                  ),\n"
                )
            }
        };
        field_widgets.push_str(&widget);
    }

    let validators_import = if cfg.validation.is_empty() {
        ""
    } else {
        "import '../validation/validators.dart';\n"
    };
    let has_dt = fields.iter().any(|g| matches!(g.kind, FieldKind::Dt));
    let fmt_import = if has_dt {
        "import '../ui/format.dart';\n"
    } else {
        ""
    };
    // _parseDt 仅在存在可空 datetime 字段时使用（必填走 tryParse ?? now）
    let has_optional_dt = fields
        .iter()
        .any(|g| matches!(g.kind, FieldKind::Dt) && !g.required);
    let loading_field = if has_update {
        "  bool _loading = false;\n"
    } else {
        ""
    };
    let loading_gate = if has_update {
        "_loading\n          ? const Center(child: CircularProgressIndicator())\n          : "
    } else {
        ""
    };
    let parse_dt = if has_optional_dt {
        "  DateTime? _parseDt(String s) {\n    final v = s.trim();\n    if (v.isEmpty) return null;\n    return DateTime.tryParse(v)?.toUtc();\n  }\n\n"
    } else {
        ""
    };

    let template = r#"// @generated by Forma (gen-flutter) — do not edit manually.
import 'package:flutter/material.dart';

import '../api/api.dart';
import '../models/models.dart';
%%VALIDATORS_IMPORT%%%%FMT_IMPORT%%
/// %%TITLE%%
class %%PAGE%% extends StatefulWidget {
  final String? id;
  const %%PAGE%%({super.key, this.id});

  @override
  State<%%PAGE%%> createState() => _%%PAGE%%State();
}

class _%%PAGE%%State extends State<%%PAGE%%> {
  final Api _api = Api();
  final GlobalKey<FormState> _formKey = GlobalKey<FormState>();
%%STATE_FIELDS%%%%LOADING_FIELD%%  String? _error;

  bool get _isCreate => %%IS_CREATE_EXPR%%;

  @override
  void initState() {
    super.initState();
%%INIT_BODY%%  }

%%LOAD_FN%%%%LOAD_OPTIONS%%%%PARSE_DT%%  Future<void> _save() async {
    if (!(_formKey.currentState?.validate() ?? false)) return;
    final input = %%ENTITY%%Input(
%%INPUT_ARGS%%    );
%%VALIDATE_BLOCK%%    try {
%%SUBMIT_BLOCK%%      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(const SnackBar(content: Text('已保存')));
      Navigator.of(context).pushReplacementNamed('%%REDIRECT%%');
    } on ApiException catch (e) {
      if (!mounted) return;
      setState(() {
        _error = e.fields.isEmpty
            ? e.message
            : e.fields.map((f) => '${f.field}: ${f.message}').join('；');
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: Text(_isCreate ? '新建%%ENTITY_LABEL%%' : '编辑%%ENTITY_LABEL%%'),
      ),
      body: %%LOADING_GATE%%Form(
              key: _formKey,
              child: ListView(
                padding: const EdgeInsets.all(16),
                children: [
                  if (_error != null)
                    Padding(
                      padding: const EdgeInsets.only(bottom: 12),
                      child: Text(
                        _error!,
                        style: const TextStyle(color: Color(0xFFDC2626)),
                      ),
                    ),
%%FIELDS%%                  const SizedBox(height: 24),
                  FilledButton(onPressed: _save, child: const Text('保存')),
                ],
              ),
            ),
    );
  }

  @override
  void dispose() {
%%DISPOSE%%    super.dispose();
  }
}
"#;

    template
        .replace("%%LOADING_FIELD%%", loading_field)
        .replace("%%LOADING_GATE%%", loading_gate)
        .replace("%%PARSE_DT%%", parse_dt)
        .replace("%%TITLE%%", &escape_dart(&title))
        .replace("%%PAGE%%", page_name)
        .replace("%%ENTITY%%", entity_name)
        .replace("%%ENTITY_LABEL%%", &escape_dart(&entity_label))
        .replace("%%VALIDATORS_IMPORT%%", validators_import)
        .replace("%%FMT_IMPORT%%", fmt_import)
        .replace("%%STATE_FIELDS%%", &state_fields)
        .replace("%%IS_CREATE_EXPR%%", is_create_expr)
        .replace("%%INIT_BODY%%", &init_body)
        .replace("%%LOAD_FN%%", &load_fn)
        .replace("%%LOAD_OPTIONS%%", &load_options)
        .replace("%%INPUT_ARGS%%", &input_args.concat())
        .replace("%%VALIDATE_BLOCK%%", &validate_block)
        .replace("%%SUBMIT_BLOCK%%", &submit_block)
        .replace("%%REDIRECT%%", &escape_dart(&redirect))
        .replace("%%FIELDS%%", &field_widgets)
        .replace("%%DISPOSE%%", &dispose_lines)
}

// ── main.dart ───────────────────────────────────────────

fn route_pattern(route: &str) -> Vec<String> {
    route
        .split('/')
        .filter(|s| !s.is_empty())
        .map(|s| {
            if s.starts_with('{') && s.ends_with('}') {
                ":id".to_string()
            } else {
                s.to_string()
            }
        })
        .collect()
}

fn pattern_lit(route: &str) -> String {
    route_pattern(route)
        .iter()
        .map(|s| format!("'{s}'"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn main_dart(program: &Program, pkg: &str) -> String {
    let app_class = format!("{}App", humanize(pkg).replace(' ', ""));

    let mut imports = String::new();
    for p in &program.pages {
        imports.push_str(&format!(
            "import 'pages/{}.dart';\n",
            to_snake_case(&p.name)
        ));
    }

    let home = program
        .pages
        .iter()
        .find(|p| matches!(p.body, PageBody::Table(_)))
        .map(|p| p.name.clone())
        .unwrap_or_default();

    let mut routes = String::new();
    let mut branches = String::new();
    for p in &program.pages {
        let has_param = p.route.contains("{id}");
        match &p.body {
            PageBody::Table(_) => {
                if !has_param {
                    routes.push_str(&format!(
                        "        '{}': (_) => const {}(),\n",
                        p.route, p.name
                    ));
                } else {
                    branches.push_str(&format!(
                        "        if (_match(segments, [{}])) {{\n          page = const {}();\n        }}\n",
                        pattern_lit(&p.route),
                        p.name
                    ));
                }
            }
            PageBody::Form(cfg) => {
                let has_update = cfg.submits.iter().any(|s| !s.is_create);
                if has_param && has_update {
                    let pat = route_pattern(&p.route);
                    let idx = pat.iter().position(|s| s == ":id").unwrap_or(1);
                    branches.push_str(&format!(
                        "        if (_match(segments, [{}])) {{\n          page = {}(id: segments[{idx}] == 'new' ? null : segments[{idx}]);\n        }}\n",
                        pattern_lit(&p.route),
                        p.name,
                        idx = idx
                    ));
                } else if has_param {
                    branches.push_str(&format!(
                        "        if (_match(segments, [{}])) {{\n          page = const {}();\n        }}\n",
                        pattern_lit(&p.route),
                        p.name
                    ));
                } else {
                    routes.push_str(&format!("        '{}': (_) => {}(),\n", p.route, p.name));
                }
            }
            PageBody::Detail(_) => {
                if has_param {
                    branches.push_str(&format!(
                        "        if (_match(segments, [{}])) {{\n          page = const {}();\n        }}\n",
                        pattern_lit(&p.route),
                        p.name
                    ));
                } else {
                    routes.push_str(&format!(
                        "        '{}': (_) => const {}(),\n",
                        p.route, p.name
                    ));
                }
            }
        }
    }

    format!(
        r#"// @generated by Forma (gen-flutter) — do not edit manually.
import 'package:flutter/material.dart';

{imports}
void main() => runApp(const {app_class}());

class {app_class} extends StatelessWidget {{
  const {app_class}({{super.key}});

  @override
  Widget build(BuildContext context) {{
    return MaterialApp(
      title: '{pkg}',
      theme: ThemeData(colorSchemeSeed: const Color(0xFF2563EB)),
      home: const {home}(),
      routes: {{
{routes}      }},
      onGenerateRoute: (settings) {{
        final segments = (settings.name ?? '/')
            .split('/')
            .where((s) => s.isNotEmpty)
            .toList();
        Widget? page;
{branches}        if (page != null) {{
          final p = page;
          return MaterialPageRoute<void>(builder: (_) => p, settings: settings);
        }}
        return null;
      }},
    );
  }}
}}

/// DSL 路由模式匹配：{{id}} → :id 通配段。
bool _match(List<String> segments, List<String> pattern) {{
  if (segments.length != pattern.length) return false;
  for (var i = 0; i < pattern.length; i++) {{
    if (pattern[i] != ':id' && pattern[i] != segments[i]) return false;
  }}
  return true;
}}
"#
    )
}

// ── test/live_api_test.dart：真实后端契约测试 ────────────

/// 生成契约测试：
/// - 离线往返：fromJson/toJson 与后端 serde 契约一致（永远可跑）；
/// - 在线实链路：需要后端在跑（未启动时自动跳过，不阻塞 CI）。
fn live_api_test_dart(program: &Program, pkg: &str) -> String {
    let first_entity = program.entities.first();
    let (entity_name, plural) = match first_entity {
        Some(e) => (e.name.clone(), to_snake_case(&super::pluralize(&e.name))),
        None => (String::new(), String::new()),
    };
    let mut offline = String::new();
    for e in &program.entities {
        let mut sample = String::from("{\n");
        for (i, f) in e.fields.iter().enumerate() {
            let key = json_key(program, f);
            let value = if f.optional {
                "null".to_string()
            } else {
                match f.type_name.as_str() {
                    "Int" => "1".to_string(),
                    "Float" => "1.5".to_string(),
                    "Bool" => "true".to_string(),
                    "Timestamp" | "Date" => "\"2025-01-06T08:00:00Z\"".to_string(),
                    "Decimal" => "\"1.00\"".to_string(),
                    other if is_enum(program, other) => {
                        let e2 = program.enums.iter().find(|x| x.name == other);
                        match e2.and_then(|x| x.variants.first()) {
                            Some(v) => format!("\"{}\"", v.name),
                            None => "null".to_string(),
                        }
                    }
                    _ => {
                        if is_relation(program, &f.type_name) {
                            "\"00000000-0000-5000-8000-000000000000\"".to_string()
                        } else {
                            "\"x\"".to_string()
                        }
                    }
                }
            };
            let comma = if i + 1 == e.fields.len() { "" } else { "," };
            sample.push_str(&format!("    '{key}': {value}{comma}\n"));
        }
        sample.push_str("  }");
        offline.push_str(&format!(
            "  test('{name} fromJson/toJson 往返（离线契约）', () {{\n    final m = {name}.fromJson({sample});\n    final back = m.toJson();\n    expect(back.length, {len});\n  }});\n\n",
            name = e.name,
            sample = sample,
            len = e.fields.len()
        ));
    }

    // 选一个可写的目标实体：有 create op 且至少一个关系字段（覆盖关系载荷）
    let write_target = program.entities.iter().find(|e| {
        program
            .apis
            .iter()
            .any(|a| a.entity == e.name && a.operations.iter().any(|o| o == "create"))
            && e.fields.iter().any(|f| is_relation(program, &f.type_name))
    });

    let write_case = match write_target {
        Some(t) => {
            let t_plural = to_snake_case(&super::pluralize(&t.name));
            let mut prep = String::new();
            let mut args = String::new();
            // 回查探针：优先取唯一字段（email / @unique），保证能定位到刚建的记录
            let probe_field = t.fields.iter().find(|f| {
                !has_attr(f, "id")
                    && !is_relation(program, &f.type_name)
                    && (f.type_name == "Email" || has_attr(f, "unique"))
            });
            let probe = match probe_field {
                Some(pf) => {
                    let fname = to_camel(&to_snake_case(&pf.name));
                    if pf.optional {
                        format!("input.{fname}!")
                    } else {
                        format!("input.{fname}")
                    }
                }
                None => "input.id".to_string(),
            };
            for f in &t.fields {
                if has_attr(f, "id") {
                    continue;
                }
                let iname = to_camel(&to_snake_case(&f.name));
                if is_relation(program, &f.type_name) {
                    let parent = &f.type_name;
                    let p_plural = to_snake_case(&super::pluralize(parent));
                    let pvar = format!("{}Rows", to_camel(&to_snake_case(parent)));
                    prep.push_str(&format!(
                        "    final {pvar} = await api.list('/{p_plural}', {p}ListRow.fromJson, const {{'page_size': '1'}});\n    expect({pvar}, isNotEmpty);\n",
                        pvar = pvar,
                        p = parent,
                        p_plural = p_plural
                    ));
                    args.push_str(&format!(
                        "      {iname}: {pvar}.first.id,\n",
                        iname = iname,
                        pvar = pvar
                    ));
                } else if !is_option_field(f) {
                    let value = match f.type_name.as_str() {
                        "Int" => "1".to_string(),
                        "Float" => "1.5".to_string(),
                        "Bool" => "true".to_string(),
                        "Decimal" => "\"1.00\"".to_string(),
                        "Timestamp" | "Date" => "DateTime.now().toUtc()".to_string(),
                        other if is_enum(program, other) => {
                            let en = program.enums.iter().find(|x| x.name == other);
                            match en.and_then(|x| x.variants.first()) {
                                Some(v) => format!("{}.{}", other, v.name),
                                None => continue,
                            }
                        }
                        _ => {
                            if f.type_name == "Email" {
                                r#""e2e-${DateTime.now().millisecondsSinceEpoch}@example.test""#
                                    .to_string()
                            } else if has_attr(f, "unique") {
                                r#""e2e-${DateTime.now().millisecondsSinceEpoch}""#.to_string()
                            } else {
                                "\"e2e\"".to_string()
                            }
                        }
                    };
                    args.push_str(&format!("      {iname}: {value},\n", iname = iname));
                }
            }
            format!(
                "  test('在线：创建→读取→更新→删除全链路（{t}）', () async {{\n    if (!await _backendUp(api)) {{\n      // ignore: avoid_print\n      print('⏭  后端未启动——跳过写入链路用例');\n      return;\n    }}\n{prep}    final input = {t}Input(\n{args}    );\n    await api.post('/{t_plural}', input.toJson());\n    // 用新建记录的唯一字段回查（实体名不具备唯一性）\n    final probe = {probe};\n    final found = await api.list('/{t_plural}', {t}ListRow.fromJson, {{'q': probe}});\n    expect(found, isNotEmpty, reason: '创建后应能查到');\n    final id = found.first.id;\n    await api.put('/{t_plural}/$id', input.toJson());\n    await api.delete('/{t_plural}/$id');\n    final after = await api.list('/{t_plural}', {t}ListRow.fromJson, {{'q': probe}});\n    expect(after, isEmpty, reason: '删除后不应再查到');\n  }});\n",
                t = t.name,
                t_plural = t_plural,
                prep = prep,
                args = args,
                probe = probe
            )
        }
        None => String::new(),
    };

    format!(
        r#"// @generated by Forma (gen-flutter) — do not edit manually.
//
// 契约测试：
//   1) 离线往返——fromJson/toJson 与 Rust 后端 serde 契约一致（永远可跑）；
//   2) 在线实链路——需要后端在跑（未启动时跳过，不阻塞 CI）。
//
// 用法：
//   cargo run -- build examples/school
//   cd examples/school/app_mobile
//   flutter test                                     # 仅离线往返
//   flutter test --dart-define=API_BASE=http://127.0.0.1:8080/api   # 含在线链路
import 'package:flutter_test/flutter_test.dart';

import 'package:{pkg}/api/api.dart';
import 'package:{pkg}/models/models.dart';

const String kBase =
    String.fromEnvironment('API_BASE', defaultValue: 'http://127.0.0.1:8080/api');

/// 后端是否可达；不可达时在线用例跳过（不阻塞 CI）。
Future<bool> _backendUp(Api api) async {{
  try {{
    await api.list('/{plural}', {entity}ListRow.fromJson, const {{'page_size': '1'}});
    return true;
  }} catch (_) {{
    return false;
  }}
}}

void main() {{
  final api = Api(baseUrl: kBase);

{offline}
  test('在线：列表解析 + 关系显示列 + 搜索', () async {{
    if (!await _backendUp(api)) {{
      // ignore: avoid_print
      print('⏭  后端未启动（$kBase）——跳过在线契约用例');
      return;
    }}
    final rows = await api.list('/{plural}', {entity}ListRow.fromJson, const {{'page_size': '5'}});
    expect(rows, isNotEmpty);
    // 搜索（后端 LIKE 语义）
    final hit = await api.list('/{plural}', {entity}ListRow.fromJson, {{'q': rows.first.id}});
    expect(hit, isNotEmpty);
  }});

{write_case}
}}
"#,
        pkg = pkg,
        plural = plural,
        entity = entity_name,
        offline = offline,
        write_case = write_case
    )
}
