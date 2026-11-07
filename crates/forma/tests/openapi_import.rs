//! OpenAPI 3.x 导入 → Forma DSL (.fm) 逆向工程回归测试（roadmap M7 / P1）。
//!
//! 验收：给定一份 OpenAPI 规范，`openapi_to_dsl` 产出可解析、可通过 `check_program`
//! 的 `.fm` 骨架，且关键类型映射（email / uuid / 关系 / 数值边界 / 操作推导）正确。
//!
//! 并行安全：产物写入独立的 tmp 目录。

use std::fs;
use std::path::Path;

fn tmp() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("forma-oaimp-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

const SPEC: &str = r#"
openapi: "3.0.0"
info:
  title: Pet Store
  version: "1.0.0"
paths:
  /pets:
    get:
      tags: [Pet]
      responses:
        '200':
          description: ok
    post:
      tags: [Pet]
      requestBody:
        content:
          application/json:
            schema:
              $ref: '#/components/schemas/Pet'
      responses:
        '201':
          description: created
  /pets/{id}:
    get:
      tags: [Pet]
      responses:
        '200':
          description: ok
    delete:
      tags: [Pet]
      responses:
        '204':
          description: gone
components:
  schemas:
    Pet:
      type: object
      required: [name]
      properties:
        id:
          type: integer
          format: int64
        name:
          type: string
        email:
          type: string
          format: email
        age:
          type: integer
          minimum: 0
          maximum: 100
        owner:
          $ref: '#/components/schemas/Owner'
        photos:
          type: array
          items:
            type: string
    Owner:
      type: object
      required: [title]
      properties:
        id:
          type: string
          format: uuid
        title:
          type: string
"#;

#[test]
fn openapi_import_generates_valid_dsl() {
    let dsl = forma::adapters::openapi::openapi_to_dsl(SPEC, "petstore.yaml").unwrap();
    let dir = tmp();
    fs::write(dir.join("app.fm"), &dsl).unwrap();

    let (program, parse_diags) = forma::dsl::load_program(&dir, &["*.fm".to_string()]).unwrap();
    let parse_err = parse_diags.iter().filter(|d| d.severity == "error").count();
    assert_eq!(
        parse_err,
        0,
        "generated .fm must parse: {:?}\n---\n{dsl}",
        parse_diags
    );

    let sem = forma::dsl::check_program(&program);
    let sem_err = sem.iter().filter(|d| d.severity == "error").count();
    assert_eq!(
        sem_err,
        0,
        "generated .fm must pass check_program: {:?}\n---\n{dsl}",
        sem
    );

    // 关键映射断言
    assert!(dsl.contains("app PetStore {"));
    assert!(dsl.contains("entity Pet {"));
    assert!(dsl.contains("entity Owner {"));
    assert!(dsl.contains("email Email"));
    assert!(dsl.contains("owner Owner"));
    assert!(dsl.contains("age Int @min(0) @max(100)"));
    // photos 是数组（v1 跳过），不应出现
    assert!(!dsl.contains("photos"));
    // Pet 操作来自 paths：list(get /pets) + create(post) + get(get /pets/{id}) + delete
    assert!(dsl.contains("operations [list get create delete]"));
    // Owner 无路径 → 默认全量 CRUD
    let owner_api = dsl
        .lines()
        .skip_while(|l| !l.starts_with("api Owner {"))
        .take(4)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        owner_api.contains("operations [list get create update delete]"),
        "Owner api should default to full CRUD:\n{owner_api}"
    );
}

#[test]
fn openapi_import_type_skeleton_is_usable() {
    // 生成的 .fm 至少能被 load_program 视为一个含两个实体、两个 api 的工程
    let dsl = forma::adapters::openapi::openapi_to_dsl(SPEC, "petstore.yaml").unwrap();
    assert!(dsl.contains("entity Pet {"));
    assert!(dsl.contains("entity Owner {"));
    assert_eq!(dsl.matches("api ").count(), 2);
    assert!(dsl.contains("locale = [\"en\"]"));
}
