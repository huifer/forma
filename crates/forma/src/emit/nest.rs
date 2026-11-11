//! NestJS / TypeORM backend target（M10 第一后端扩栈）：有限但真实可运行的子集。
//!
//! 支持：实体标量字段、CRUD、offset list、字段 min/max/required DTO 校验。
//! 目前显式拒绝：auth/access/tenant/commands/跨字段 rules（不能静默降级安全语义）。

use crate::dsl::{has_attr, is_enum, is_relation, FieldDecl, Program};
use crate::generators::GeneratedFile;
use crate::identity::to_snake_case;
use std::path::Path;

pub fn emit_nestjs(
    project_dir: &Path,
    program: &Program,
    out_dir: &str,
    app_name: &str,
) -> anyhow::Result<Vec<GeneratedFile>> {
    if program.entities.is_empty() {
        return Ok(vec![]);
    }
    reject_unsupported(program)?;
    let mut files = Vec::new();
    let pkg = to_snake_case(app_name).replace('_', "-");

    files.push(file(out_dir, "package.json", package_json(&pkg)));
    files.push(file(out_dir, "tsconfig.json", tsconfig()));
    files.push(file(out_dir, "nest-cli.json", nest_cli()));
    files.push(file(out_dir, "src/main.ts", main_ts()));
    files.push(file(out_dir, "src/app.module.ts", app_module(program)));
    // 枚举类型声明（entity / DTO 引用）
    if !program.enums.is_empty() {
        files.push(file(out_dir, "src/enums.ts", enums_ts(program)));
    }
    for entity in &program.entities {
        files.push(file(
            out_dir,
            &format!(
                "src/{}/{}.entity.ts",
                to_snake_case(&entity.name),
                to_snake_case(&entity.name)
            ),
            entity_source(program, entity),
        ));
        files.push(file(
            out_dir,
            &format!(
                "src/{}/{}.dto.ts",
                to_snake_case(&entity.name),
                to_snake_case(&entity.name)
            ),
            dto_source(program, entity),
        ));
        files.push(file(
            out_dir,
            &format!(
                "src/{}/{}.service.ts",
                to_snake_case(&entity.name),
                to_snake_case(&entity.name)
            ),
            service_source(program, entity),
        ));
        files.push(file(
            out_dir,
            &format!(
                "src/{}/{}.controller.ts",
                to_snake_case(&entity.name),
                to_snake_case(&entity.name)
            ),
            controller_source(entity),
        ));
        files.push(file(
            out_dir,
            &format!(
                "src/{}/{}.module.ts",
                to_snake_case(&entity.name),
                to_snake_case(&entity.name)
            ),
            module_source(entity),
        ));
    }
    files.push(file(
        out_dir,
        "README.md",
        format!(
            "{}{}",
            crate::emit::generated_md_header("gen-nestjs"),
            readme(app_name)
        ),
    ));
    for generated in &files {
        crate::util::write_text(&project_dir.join(&generated.path), &generated.content)?;
    }
    Ok(files)
}

fn reject_unsupported(program: &Program) -> anyhow::Result<()> {
    let mut unsupported = Vec::new();
    if program.auth.is_some() {
        unsupported.push("auth module (JWT/RBAC)");
    }
    if !program.commands.is_empty() {
        unsupported.push("command operations");
    }
    if program.rules.iter().any(|r| !r.access.is_empty()) {
        unsupported.push("access/row/field policies");
    }
    if program
        .rules
        .iter()
        .any(|r| !r.checks.is_empty() || !r.guards.is_empty())
    {
        unsupported.push("cross-field checks/guards");
    }
    if program
        .entities
        .iter()
        .flat_map(|e| &e.fields)
        .any(|f| has_attr(f, "tenant"))
    {
        unsupported.push("tenant isolation");
    }
    if !unsupported.is_empty() {
        anyhow::bail!(
            "NestJS target currently supports scalar CRUD + field min/max/required only; unsupported DSL features: {}. Select backend.language: rust to preserve full semantics",
            unsupported.join(", ")
        );
    }
    Ok(())
}

fn file(out_dir: &str, rel: &str, content: String) -> GeneratedFile {
    GeneratedFile {
        path: format!("{out_dir}/{rel}"),
        content,
        generator: "gen-nestjs".into(),
    }
}

fn package_json(pkg: &str) -> String {
    format!(
        r#"{{
  "name": "{pkg}-backend",
  "version": "0.1.0",
  "private": true,
  "scripts": {{
    "build": "nest build",
    "start": "node dist/main.js",
    "start:dev": "nest start --watch",
    "test": "jest"
  }},
  "dependencies": {{
    "@nestjs/common": "^10.4.0",
    "@nestjs/core": "^10.4.0",
    "@nestjs/platform-express": "^10.4.0",
    "@nestjs/typeorm": "^10.0.2",
    "class-transformer": "^0.5.1",
    "class-validator": "^0.14.1",
    "mysql2": "^3.11.0",
    "reflect-metadata": "^0.2.2",
    "rxjs": "^7.8.1",
    "typeorm": "^0.3.20"
  }},
  "devDependencies": {{
    "@nestjs/cli": "^10.4.0",
    "@nestjs/schematics": "^10.1.0",
    "@nestjs/testing": "^10.4.0",
    "@types/express": "^5.0.0",
    "@types/node": "^22.0.0",
    "typescript": "^5.5.4"
  }}
}}
"#
    )
}

fn tsconfig() -> String {
    r#"{
  "compilerOptions": {
    "module": "commonjs",
    "declaration": true,
    "removeComments": true,
    "emitDecoratorMetadata": true,
    "experimentalDecorators": true,
    "allowSyntheticDefaultImports": true,
    "target": "ES2021",
    "sourceMap": true,
    "outDir": "./dist",
    "baseUrl": "./",
    "incremental": true,
    "skipLibCheck": true,
    "strictNullChecks": true,
    "noImplicitAny": false,
    "strictBindCallApply": false,
    "forceConsistentCasingInFileNames": true,
    "noFallthroughCasesInSwitch": true
  },
  "include": ["src/**/*.ts"]
}
"#
    .to_string()
}

fn nest_cli() -> String {
    r#"{
  "collection": "@nestjs/schematics",
  "sourceRoot": "src",
  "compilerOptions": { "deleteOutDir": true }
}
"#
    .to_string()
}

fn main_ts() -> String {
    r#"import "reflect-metadata";
import { ValidationPipe } from "@nestjs/common";
import { NestFactory } from "@nestjs/core";
import { AppModule } from "./app.module";

async function bootstrap(): Promise<void> {
  const app = await NestFactory.create(AppModule);
  app.setGlobalPrefix("api");
  app.useGlobalPipes(new ValidationPipe({ whitelist: true, transform: true }));
  const port = Number(process.env.PORT ?? 8080);
  await app.listen(port, "0.0.0.0");
  console.log(`backend listening on http://0.0.0.0:${port}/api`);
}
void bootstrap();
"#
    .to_string()
}

fn app_module(program: &Program) -> String {
    let mut imports = String::new();
    let mut entities = Vec::new();
    let mut modules = Vec::new();
    for e in &program.entities {
        let snake = to_snake_case(&e.name);
        let pascal = &e.name;
        imports.push_str(&format!(
            "import {{ {pascal}Module }} from \"./{snake}/{snake}.module\";\n"
        ));
        imports.push_str(&format!(
            "import {{ {pascal} }} from \"./{snake}/{snake}.entity\";\n"
        ));
        entities.push(pascal.clone());
        modules.push(format!("{pascal}Module"));
    }
    format!(
        r#"import {{ Module }} from "@nestjs/common";
import {{ TypeOrmModule }} from "@nestjs/typeorm";
{imports}
@Module({{
  imports: [
    TypeOrmModule.forRoot({{
      type: "mysql",
      host: process.env.DB_HOST ?? "localhost",
      port: Number(process.env.DB_PORT ?? 3306),
      username: process.env.DB_USER ?? "root",
      password: process.env.DB_PASS ?? "root",
      database: process.env.DB_NAME ?? "{db}",
      entities: [{entities}],
      synchronize: false,
    }}),
    {modules}
  ],
}})
export class AppModule {{}}
"#,
        db = to_snake_case(
            program
                .app
                .as_ref()
                .map(|a| a.name.as_str())
                .unwrap_or("app")
        ),
        entities = entities.join(", "),
        modules = modules.join(", "),
    )
}

/// 枚举 → TS 联合类型（与 web_react 的 types.ts 同形：string 字面量联合）。
/// 实体 / DTO 直接引用该类型，必须先声明（否则 TS2304）。
fn enums_ts(program: &Program) -> String {
    let mut s = String::from("// @generated by Forma (gen-nestjs) — do not edit manually.\n\n");
    for e in &program.enums {
        let variants: Vec<String> = e
            .variants
            .iter()
            .map(|v| format!("\"{}\"", v.name))
            .collect();
        s.push_str(&format!(
            "export type {} = {};\n",
            e.name,
            variants.join(" | ")
        ));
    }
    s
}

/// 实体 / DTO 引用的枚举类型（按使用到的枚举去重）。
fn enum_imports(program: &Program, entity: &crate::dsl::EntityDecl) -> (String, Vec<String>) {
    let mut used: Vec<String> = Vec::new();
    for f in &entity.fields {
        if is_enum(program, &f.type_name) && !used.contains(&f.type_name) {
            used.push(f.type_name.clone());
        }
    }
    if used.is_empty() {
        (String::new(), used)
    } else {
        (
            format!("import {{ {} }} from \"../enums\";\n", used.join(", ")),
            used,
        )
    }
}

fn entity_source(program: &Program, entity: &crate::dsl::EntityDecl) -> String {
    let mut fields = String::new();
    for f in &entity.fields {
        let name = to_snake_case(&f.name);
        let ty = ts_field_type(program, f);
        let column_type = typeorm_type(program, f);
        if has_attr(f, "id") {
            fields.push_str(&format!(
                "  @PrimaryColumn({column_type})\n  {name}: {ty};\n\n"
            ));
        } else {
            // 可空判定：DSL 标记 optional 或带 default/readonly（与列 nullable 一致）。
            // 注意：TS 的可选性只体现在属性名 `name?: T`，类型本身不能再带 `?`
            // （`T?` 是非法 TS 语法 → TS17019）。
            let nullable = f.optional || has_attr(f, "default") || has_attr(f, "readonly");
            fields.push_str(&format!(
                "  @Column({{ ...{column_type}, nullable: {nullable} }})\n  {name}{optional}: {ty};\n\n",
                nullable = nullable,
                optional = if nullable { "?" } else { "" },
            ));
        }
    }
    let (enum_import, _) = enum_imports(program, entity);
    format!(
        r#"import {{ Column, Entity, PrimaryColumn }} from "typeorm";
{enum_import}
@Entity("{table}")
export class {name} {{
{fields}}}
"#,
        enum_import = enum_import,
        table = crate::dsl::pluralize(&entity.name).to_lowercase(),
        name = entity.name,
        fields = fields,
    )
}

fn typeorm_type(program: &Program, f: &FieldDecl) -> String {
    let ty = if is_relation(program, &f.type_name) {
        "varchar"
    } else if is_enum(program, &f.type_name) {
        "enum"
    } else {
        match f.type_name.as_str() {
            "Uuid" => "varchar",
            "String" | "Email" => "varchar",
            "Text" => "text",
            "Int" => "bigint",
            "Float" => "double",
            "Decimal" => "decimal",
            "Bool" => "boolean",
            "Timestamp" => "datetime",
            "Date" => "date",
            "Json" => "json",
            _ => "varchar",
        }
    };
    match f.type_name.as_str() {
        "String" => format!(
            "{{ type: \"varchar\", length: {} }}",
            f.type_args.first().cloned().unwrap_or_else(|| "255".into())
        ),
        "Decimal" => format!(
            "{{ type: \"decimal\", precision: {}, scale: {} }}",
            f.type_args.first().cloned().unwrap_or_else(|| "12".into()),
            f.type_args.get(1).cloned().unwrap_or_else(|| "2".into())
        ),
        other if is_enum(program, other) => {
            let values = program
                .enums
                .iter()
                .find(|e| e.name == other)
                .map(|e| {
                    e.variants
                        .iter()
                        .map(|v| format!("\"{}\"", v.name))
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            format!("{{ type: \"enum\", enum: [{values}] }}")
        }
        "Uuid" | "Email" => format!(
            "{{ type: \"varchar\", length: {} }}",
            if f.type_name == "Email" { "320" } else { "36" }
        ),
        _ => format!("{{ type: \"{ty}\" }}"),
    }
}

fn ts_field_type(program: &Program, f: &FieldDecl) -> String {
    if is_relation(program, &f.type_name) {
        return "string".into();
    }
    if is_enum(program, &f.type_name) {
        return f.type_name.clone();
    }
    match f.type_name.as_str() {
        "Int" | "Float" | "Decimal" => "number".into(),
        "Bool" => "boolean".into(),
        "Json" => "Record<string, unknown>".into(),
        _ => "string".into(),
    }
}

fn dto_source(program: &Program, entity: &crate::dsl::EntityDecl) -> String {
    let mut fields = String::new();
    for f in &entity.fields {
        if has_attr(f, "id") || has_attr(f, "readonly") {
            continue;
        }
        let name = to_snake_case(&f.name);
        let ty = ts_field_type(program, f);
        let optional = f.optional || has_attr(f, "default");
        if optional {
            fields.push_str("  @IsOptional()\n");
        } else if matches!(f.type_name.as_str(), "String" | "Email" | "Text") {
            fields.push_str("  @IsString()\n");
        }
        if f.type_name == "Email" {
            fields.push_str("  @IsEmail()\n");
        }
        if f.type_name == "Int" {
            fields.push_str("  @IsInt()\n");
        }
        for attr in &f.attrs {
            if matches!(attr.name.as_str(), "min" | "max") {
                if let Some(v) = attr.args.first() {
                    fields.push_str(&format!(
                        "  @{}({v})\n",
                        if attr.name == "min" { "Min" } else { "Max" }
                    ));
                }
            }
        }
        fields.push_str(&format!(
            "  {name}{}: {ty};\n\n",
            if optional { "?" } else { "" }
        ));
    }
    let (enum_import, _) = enum_imports(program, entity);
    format!(
        "import {{ IsEmail, IsInt, IsOptional, IsString, Max, Min }} from \"class-validator\";\n{enum_import}\nexport class Create{name}Dto {{\n{fields}}}\n\nexport class Update{name}Dto extends Create{name}Dto {{}}\n",
        enum_import = enum_import,
        name = entity.name,
        fields = fields,
    )
}

fn service_source(program: &Program, entity: &crate::dsl::EntityDecl) -> String {
    let name = &entity.name;
    let snake = to_snake_case(name);

    let api = program.apis.iter().find(|a| a.entity == *name);
    let has = |op: &str| api.is_some_and(|a| a.operations.iter().any(|x| x == op));
    let mut methods = String::new();
    if has("list") {
        methods.push_str(
            "  list(page = 1, pageSize = 20) { return this.repo.find({ skip: (page - 1) * pageSize, take: Math.min(pageSize, 200) }); }\n",
        );
    }
    if has("get") {
        methods.push_str(
            "  async get(id: string) { const row = await this.repo.findOneBy({ id }); if (!row) throw new NotFoundException(); return row; }\n",
        );
    }
    if has("create") {
        methods.push_str(&format!(
            "  create(dto: Create{name}Dto) {{ return this.repo.save(this.repo.create(dto)); }}\n"
        ));
    }
    if has("update") {
        methods.push_str(&format!(
            "  async update(id: string, dto: Update{name}Dto) {{ const row = await this.get(id); Object.assign(row, dto); return this.repo.save(row); }}\n"
        ));
    }
    if has("delete") {
        methods.push_str(
            "  async remove(id: string) { const row = await this.get(id); await this.repo.remove(row); return { ok: true }; }\n"
        );
    }
    format!(
        "import {{ Injectable, NotFoundException }} from \"@nestjs/common\";\nimport {{ InjectRepository }} from \"@nestjs/typeorm\";\nimport {{ Repository }} from \"typeorm\";\nimport {{ {name} }} from \"./{snake}.entity\";\nimport {{ Create{name}Dto, Update{name}Dto }} from \"./{snake}.dto\";\n\n@Injectable()\nexport class {name}Service {{\n  constructor(@InjectRepository({name}) private readonly repo: Repository<{name}>) {{}}\n{methods}}}\n",
        name = name,
        snake = snake,
        methods = methods,
    )
}

fn controller_source(entity: &crate::dsl::EntityDecl) -> String {
    let name = &entity.name;
    let snake = to_snake_case(name);
    let plural = crate::dsl::pluralize(name).to_lowercase();

    let api = "";
    let _ = api;
    // Controller declares the conventional CRUD endpoints; service only emits declared methods.
    format!(
        "import {{ Body, Controller, Delete, Get, Param, Post, Put, Query }} from \"@nestjs/common\";\nimport {{ {name}Service }} from \"./{snake}.service\";\nimport {{ Create{name}Dto, Update{name}Dto }} from \"./{snake}.dto\";\n\n@Controller(\"{plural}\")\nexport class {name}Controller {{\n  constructor(private readonly service: {name}Service) {{}}\n  @Get() list(@Query(\"page\") page?: string, @Query(\"pageSize\") pageSize?: string) {{ return this.service.list(Number(page ?? 1), Number(pageSize ?? 20)); }}\n  @Get(\":id\") get(@Param(\"id\") id: string) {{ return this.service.get(id); }}\n  @Post() create(@Body() dto: Create{name}Dto) {{ return this.service.create(dto); }}\n  @Put(\":id\") update(@Param(\"id\") id: string, @Body() dto: Update{name}Dto) {{ return this.service.update(id, dto); }}\n  @Delete(\":id\") remove(@Param(\"id\") id: string) {{ return this.service.remove(id); }}\n}}\n",
        name = name,
        snake = snake,
        plural = plural,
    )
}

fn module_source(entity: &crate::dsl::EntityDecl) -> String {
    let name = &entity.name;
    let snake = to_snake_case(name);
    format!(
        "import {{ Module }} from \"@nestjs/common\";\nimport {{ TypeOrmModule }} from \"@nestjs/typeorm\";\nimport {{ {name} }} from \"./{snake}.entity\";\nimport {{ {name}Service }} from \"./{snake}.service\";\nimport {{ {name}Controller }} from \"./{snake}.controller\";\n\n@Module({{ imports: [TypeOrmModule.forFeature([{name}])], controllers: [{name}Controller], providers: [{name}Service] }})\nexport class {name}Module {{}}\n"
    )
}

fn readme(app: &str) -> String {
    format!(
        "# {app} NestJS backend (generated by Forma)\n\n## Quick start\n\n```bash\nnpm install\n# Set DB_HOST / DB_USER / DB_PASS / DB_NAME / PORT\nnpm run start:dev\n```\n\nAPI prefix: `/api`\n\n> Schema synchronization is disabled (`synchronize: false`). Apply the generated SQL migration under the configured database target before starting the service.\n"
    )
}
