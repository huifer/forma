//! Next.js（App Router / SSR 变体）target（M10）：Program → 可运行的 Next.js 工程。
//!
//! 与 web-react 同构地复用同一份 TypeScript「共享层」：
//! - `src/types.ts` ← web_react::types_ts
//! - `src/validators.ts` ← web_react::validators_ts（客户端表单校验直接复用）
//! - `src/format.ts` ← web_react::format_ts
//!
//! SSR 形态（与 SPA 的关键分叉点）：
//! - 列表页 = **Server Component**（服务端直连 REST，`src/api-server.ts` + `API_BASE` env）；
//! - 变更 = **Server Actions**（`app/actions.ts`，"use server"；表单是客户端组件调用 action）；
//! - `next.config.mjs` 把 `/api/*` rewrite 到后端（客户端相对路径也可用）。
//!
//! v1 范围：实体驱动 CRUD 路由 + 空 input 命令按钮；auth UI / 带 input 命令对话框留后续。

use crate::dsl::{is_relation, pluralize, Program};
use crate::generators::GeneratedFile;
use crate::identity::to_snake_case;
use std::path::Path;

pub fn emit_nextjs(
    project_dir: &Path,
    program: &Program,
    out_dir: &str,
    app_name: &str,
) -> anyhow::Result<Vec<GeneratedFile>> {
    if program.entities.is_empty() {
        return Ok(vec![]);
    }
    let title = program
        .app
        .as_ref()
        .map(|a| a.name.clone())
        .unwrap_or_else(|| app_name.to_string());

    let mut files: Vec<GeneratedFile> = vec![
        file(out_dir, "package.json", package_json(app_name)),
        file(out_dir, "next.config.mjs", next_config()),
        file(out_dir, "tsconfig.json", tsconfig()),
        file(out_dir, "src/api-server.ts", api_server_ts(program)),
        file(
            out_dir,
            "src/api.ts",
            crate::emit::web_react::api_ts(program),
        ),
        file(
            out_dir,
            "src/types.ts",
            crate::emit::web_react::types_ts(program),
        ),
    ];
    files.push(file(
        out_dir,
        "src/validators.ts",
        crate::emit::web_react::validators_ts(program),
    ));
    files.push(file(
        out_dir,
        "src/validators.ts",
        crate::emit::web_react::validators_ts(program),
    ));
    files.push(file(
        out_dir,
        "src/format.ts",
        crate::emit::web_react::format_ts(program),
    ));
    files.push(file(out_dir, "app/layout.tsx", layout_tsx(program, &title)));
    files.push(file(out_dir, "app/page.tsx", home_tsx(program, &title)));
    files.push(file(out_dir, "app/actions.ts", actions_ts(program)));
    for api in &program.apis {
        let Some(entity) = crate::dsl::find_entity(program, &api.entity) else {
            continue;
        };
        let plural = to_snake_case(&pluralize(&entity.name));
        let columns = list_columns(program, entity);
        files.push(file(
            out_dir,
            &format!("app/{plural}/page.tsx"),
            list_page_tsx(program, entity, api, &columns),
        ));
        // new/edit 页按 api operations 裁剪（VoteRecord 只有 create 就没有编辑页）
        if api.operations.iter().any(|o| o == "create") {
            files.push(file(
                out_dir,
                &format!("app/{plural}/new/page.tsx"),
                new_page_tsx(program, entity),
            ));
        }
        if api.operations.iter().any(|o| o == "update") {
            files.push(file(
                out_dir,
                &format!("app/{plural}/[id]/page.tsx"),
                edit_page_tsx(program, entity),
            ));
        }
    }
    files.push(file(out_dir, ".github/workflows/ci.yml", ci_yaml()));
    files.push(file(out_dir, "README.md", readme(&title)));

    for f in &files {
        crate::util::write_text(&project_dir.join(&f.path), &f.content)?;
    }
    Ok(files)
}

fn file(out: &str, rel: &str, content: String) -> GeneratedFile {
    GeneratedFile {
        path: format!("{out}/{rel}"),
        content,
        generator: "gen-nextjs".into(),
    }
}

/// 列表列：优先取该实体的 table 页 columns；否则全字段 snake 名。
fn list_columns(program: &Program, entity: &crate::dsl::EntityDecl) -> Vec<String> {
    for page in &program.pages {
        if let crate::dsl::PageBody::Table(cfg) = &page.body {
            if let Some(api) = program.apis.iter().find(|a| a.name == cfg.query_api) {
                if api.entity == entity.name {
                    return cfg
                        .columns
                        .iter()
                        .map(|c| to_snake_case(c.path.rsplit('.').next().unwrap_or(&c.path)))
                        .collect();
                }
            }
        }
    }
    entity
        .fields
        .iter()
        .map(|f| to_snake_case(&f.name))
        .collect()
}

/// 列的 JSON 键（ListRow 契约：关系 → `{rel}_name`，标量 → snake 名）。
fn column_json_key(program: &Program, entity: &crate::dsl::EntityDecl, col: &str) -> String {
    if let Some(f) = entity.fields.iter().find(|f| to_snake_case(&f.name) == col) {
        if is_relation(program, &f.type_name) {
            return format!("{col}_name");
        }
    }
    col.to_string()
}

fn input_type_of(type_name: &str) -> &'static str {
    match type_name {
        "Int" | "Float" => "number",
        _ => "text",
    }
}

fn package_json(app_name: &str) -> String {
    let pkg = to_snake_case(app_name).replace('_', "-");
    format!("{{ \"name\": \"{pkg}-web-next\", \"private\": true, \"version\": \"0.1.0\", \"scripts\": {{ \"dev\": \"next dev\", \"build\": \"next build\", \"start\": \"next start\", \"tsc\": \"tsc --noEmit\" }}, \"dependencies\": {{ \"next\": \"14.2.5\", \"react\": \"^18.3.1\", \"react-dom\": \"^18.3.1\" }}, \"devDependencies\": {{ \"@types/node\": \"^20.0.0\", \"@types/react\": \"^18.3.0\", \"@types/react-dom\": \"^18.3.0\", \"typescript\": \"^5.5.4\" }} }}")
}

fn next_config() -> String {
    let mut s = String::new();
    s.push_str("/** @type {import('next').NextConfig} */\n");
    s.push_str("const API_BASE = process.env.API_BASE ?? \"http://localhost:8080\";\n\n");
    s.push_str("const nextConfig = {\n");
    s.push_str("  async rewrites() {\n");
    s.push_str(
        "    return [{ source: \"/api/:path*\", destination: `${API_BASE}/api/:path*` }];\n",
    );
    s.push_str("  },\n};\n\n");
    s.push_str("export default nextConfig;\n");
    s
}

fn tsconfig() -> String {
    let mut s = String::new();
    s.push_str("{\n  \"compilerOptions\": {\n");
    s.push_str(
        "    \"target\": \"ES2020\",\n    \"lib\": [\"dom\", \"dom.iterable\", \"esnext\"],\n",
    );
    s.push_str("    \"strict\": true,\n    \"noEmit\": true,\n    \"esModuleInterop\": true,\n");
    s.push_str("    \"module\": \"esnext\",\n    \"moduleResolution\": \"bundler\",\n");
    s.push_str("    \"resolveJsonModule\": true,\n    \"isolatedModules\": true,\n");
    s.push_str(
        "    \"jsx\": \"preserve\",\n    \"skipLibCheck\": true,\n    \"incremental\": true,\n",
    );
    s.push_str("    \"plugins\": [{ \"name\": \"next\" }]\n  },\n");
    s.push_str("  \"include\": [\"next-env.d.ts\", \"**/*.ts\", \"**/*.tsx\"],\n");
    s.push_str("  \"exclude\": [\"node_modules\"]\n}\n");
    s
}

/// 服务端 REST 客户端：Server Components 与 Server Actions 的数据来源。
fn api_server_ts(program: &Program) -> String {
    let mut s = String::new();
    s.push_str("// @generated by Forma (gen-nextjs) — do not edit manually.\n");
    s.push_str(
        "// Server Component / Server Action 专用（绝对地址；客户端组件请走 Server Actions）。\n",
    );
    s.push_str("// API_BASE 环境变量指定后端地址（默认 http://localhost:8080）；API_TOKEN 透传 bearer。\n\n");
    let mut type_imports: Vec<String> = Vec::new();
    for e in &program.entities {
        type_imports.push(e.name.clone());
        type_imports.push(format!("{}ListRow", e.name));
    }
    s.push_str("import { ");
    s.push_str(&type_imports.join(", "));
    s.push_str(" } from \"./types\";\n\n");
    s.push_str("const base = process.env.API_BASE ?? \"http://localhost:8080\";\n\n");
    s.push_str("async function req<T>(path: string, init?: RequestInit): Promise<T> {\n");
    s.push_str("  const headers: Record<string, string> = { \"Content-Type\": \"application/json\", ...(init?.headers as Record<string, string> | undefined) };\n");
    s.push_str("  const token = process.env.API_TOKEN;\n");
    s.push_str("  if (token) headers[\"Authorization\"] = `Bearer ${token}`;\n");
    s.push_str(
        "  const res = await fetch(`${base}${path}`, { ...init, headers, cache: \"no-store\" });\n",
    );
    s.push_str("  if (!res.ok) {\n    const body = await res.json().catch(() => ({}) as any);\n    const err = new Error(body.error ?? res.statusText) as any;\n    err.status = res.status;\n    err.code = body.code;\n    throw err;\n  }\n");
    s.push_str("  if (res.status === 204) return undefined as T;\n  return res.json() as Promise<T>;\n}\n\n");
    for entity in &program.entities {
        let name = &entity.name;
        let plural = to_snake_case(&pluralize(name));
        s.push_str("export const ");
        s.push_str(name);
        s.push_str("Api = {\n");
        s.push_str("  list: (p: Record<string, unknown> = {}) =>\n    req<");
        s.push_str(name);
        s.push_str("ListRow[]>(`/api/");
        s.push_str(&plural);
        s.push_str("?${new URLSearchParams(Object.entries(p).filter(([, v]) => v !== null && v !== undefined && v !== \"\").map(([k, v]) => [k, String(v)] as [string, string]))}`),\n");
        s.push_str("  get: async (id: string) => req<");
        s.push_str(name);
        s.push_str(">(`/api/");
        s.push_str(&plural);
        s.push_str("/` + id),\n");
        s.push_str("  create: (input: unknown) => req<");
        s.push_str(name);
        s.push_str(">(`/api/");
        s.push_str(&plural);
        s.push_str("`, { method: \"POST\", body: JSON.stringify(input) }),\n");
        s.push_str("  update: (id: string, input: unknown) => req<");
        s.push_str(name);
        s.push_str(">(`/api/");
        s.push_str(&plural);
        s.push_str("/` + id, { method: \"PUT\", body: JSON.stringify(input) }),\n");
        s.push_str("  remove: async (id: string) => {\n    await req(`/api/");
        s.push_str(&plural);
        s.push_str("/` + id, { method: \"DELETE\" });\n    return { ok: true };\n  },\n};\n\n");
    }
    s
}

fn layout_tsx(program: &Program, title: &str) -> String {
    let mut s = String::new();
    s.push_str("// @generated by Forma (gen-nextjs) — do not edit manually.\n");
    s.push_str("import Link from \"next/link\";\n");
    s.push_str("import React from \"react\";\n\n");
    s.push_str("const NAV: { href: string; label: string }[] = [\n");
    let mut seen: Vec<String> = Vec::new();
    for api in &program.apis {
        let Some(entity) = crate::dsl::find_entity(program, &api.entity) else {
            continue;
        };
        let plural = to_snake_case(&pluralize(&entity.name));
        if seen.contains(&plural) {
            continue;
        }
        seen.push(plural.clone());
        s.push_str("  { href: \"/");
        s.push_str(&plural);
        s.push_str("\", label: \"");
        s.push_str(&entity.name);
        s.push_str("\" },\n");
    }
    s.push_str("];\n\n");
    s.push_str(
        "export default function RootLayout({ children }: { children: React.ReactNode }) {\n",
    );
    s.push_str("  return (\n    <html lang=\"zh\">\n      <body style={{ margin: 0, fontFamily: \"system-ui, sans-serif\" }}>\n");
    s.push_str("        <header style={{ padding: \"12px 24px\", borderBottom: \"1px solid #eee\", display: \"flex\", gap: 16, alignItems: \"baseline\" }}>\n");
    s.push_str("          <strong>");
    s.push_str(title);
    s.push_str("</strong>\n");
    s.push_str("          {NAV.map((n) => (\n            <Link key={n.href} href={n.href} style={{ color: \"#06c\" }}>{n.label}</Link>\n          ))}\n");
    s.push_str("        </header>\n        <main style={{ padding: 24 }}>{children}</main>\n      </body>\n    </html>\n  );\n}\n");
    s
}

fn home_tsx(program: &Program, title: &str) -> String {
    let mut s = String::new();
    s.push_str("// @generated by Forma (gen-nextjs) — do not edit manually.\n");
    s.push_str("import Link from \"next/link\";\n\n");
    s.push_str("export default function Home() {\n");
    s.push_str("  return (\n    <div>\n      <h1>");
    s.push_str(title);
    s.push_str("</h1>\n      <ul>\n");
    for api in &program.apis {
        let Some(entity) = crate::dsl::find_entity(program, &api.entity) else {
            continue;
        };
        let plural = to_snake_case(&pluralize(&entity.name));
        s.push_str("        <li key=\"");
        s.push_str(&plural);
        s.push_str("\"><Link href=\"/");
        s.push_str(&plural);
        s.push_str("\">");
        s.push_str(&entity.name);
        s.push_str(" 列表</Link></li>\n");
    }
    s.push_str("      </ul>\n    </div>\n  );\n}\n");
    s
}

/// Server Actions：每实体 CRUD + 命令（全部 "use server"）。
fn actions_ts(program: &Program) -> String {
    let mut s = String::new();
    s.push_str("\"use server\";\n\n");
    s.push_str("// @generated by Forma (gen-nextjs) — do not edit manually.\n");
    s.push_str("import { revalidatePath } from \"next/cache\";\n");
    for entity in &program.entities {
        let plural = to_snake_case(&pluralize(&entity.name));
        s.push_str("import { ");
        s.push_str(&entity.name);
        s.push_str("Api } from \"../src/api-server\";\n");
        let _ = plural;
    }
    s.push('\n');
    for api in &program.apis {
        let Some(entity) = crate::dsl::find_entity(program, &api.entity) else {
            continue;
        };
        let name = &entity.name;
        let plural = to_snake_case(&pluralize(name));
        if api.operations.iter().any(|o| o == "create") {
            s.push_str("export async function create");
            s.push_str(name);
            s.push_str("(input: unknown) {\n  await ");
            s.push_str(name);
            s.push_str("Api.create(input);\n  revalidatePath(\"/");
            s.push_str(&plural);
            s.push_str("\");\n}\n\n");
        }
        if api.operations.iter().any(|o| o == "update") {
            s.push_str("export async function update");
            s.push_str(name);
            s.push_str("(id: string, input: unknown) {\n  await ");
            s.push_str(name);
            s.push_str("Api.update(id, input);\n  revalidatePath(\"/");
            s.push_str(&plural);
            s.push_str("\");\n}\n\n");
        }
        if api.operations.iter().any(|o| o == "delete") {
            s.push_str("export async function delete");
            s.push_str(name);
            s.push_str("(id: string) {\n  await ");
            s.push_str(name);
            s.push_str("Api.remove(id);\n  revalidatePath(\"/");
            s.push_str(&plural);
            s.push_str("\");\n}\n\n");
        }
        if api.operations.iter().any(|o| o == "get") {
            s.push_str("export async function get");
            s.push_str(name);
            s.push_str("(id: string) {\n  return ");
            s.push_str(name);
            s.push_str("Api.get(id);\n}\n\n");
        }
    }
    // 命令：{cmd_snake}(id[, input])；UI 只接空 input 命令按钮
    for cmd in &program.commands {
        let Some(effect_entity) = cmd.effects.first().map(|e| e.entity.clone()) else {
            continue;
        };
        let plural = to_snake_case(&pluralize(&effect_entity));
        let fn_name = to_snake_case(&cmd.name);
        if cmd.input.is_empty() {
            s.push_str("export async function ");
            s.push_str(&fn_name);
            s.push_str("(id: string) {\n  const res = await fetch(`${process.env.API_BASE ?? \"http://localhost:8080\"}/api");
            s.push_str(&cmd.path);
            s.push_str("`.replace(\"{id}\", id), { method: \"POST\", headers: { \"Content-Type\": \"application/json\" }, body: \"{}\" });\n");
            s.push_str("  if (!res.ok) {\n    const body = await res.json().catch(() => ({}) as any);\n    throw new Error(body.error ?? \"command failed\");\n  }\n");
            s.push_str("  revalidatePath(\"/");
            s.push_str(&plural);
            s.push_str("\");\n}\n\n");
        }
    }
    s
}

/// 列表页：Server Component，直连 REST。
fn list_page_tsx(
    program: &Program,
    entity: &crate::dsl::EntityDecl,
    api: &crate::dsl::ApiDecl,
    columns: &[String],
) -> String {
    let name = &entity.name;
    let plural = to_snake_case(&pluralize(name));
    let title = program
        .pages
        .iter()
        .find(|p| matches!(&p.body, crate::dsl::PageBody::Table(cfg) if cfg.query_api == api.name))
        .map(|p| crate::dsl::pick_message(&p.title, &p.name))
        .unwrap_or_else(|| name.clone());

    let mut s = String::new();
    s.push_str("// @generated by Forma (gen-nextjs) — do not edit manually.\n");
    s.push_str("import Link from \"next/link\";\n");
    s.push_str("import { ");
    s.push_str(name);
    s.push_str("Api } from \"../../src/api-server\";\n");
    // 命令 action 导入（空 input 命令）
    let cmds: Vec<&crate::dsl::CommandDecl> = program
        .commands
        .iter()
        .filter(|c| {
            c.input.is_empty() && c.effects.first().map(|e| e.entity.clone()) == Some(name.clone())
        })
        .collect();
    for cmd in &cmds {
        s.push_str("import { ");
        s.push_str(&to_snake_case(&cmd.name));
        s.push_str(" } from \"../../app/actions\";\n");
    }
    if api.operations.iter().any(|o| o == "delete") {
        s.push_str("import { delete");
        s.push_str(name);
        s.push_str(" } from \"../../app/actions\";\n");
    }
    s.push_str("\nexport const dynamic = \"force-dynamic\";\n\n");
    s.push_str("export default async function ");
    s.push_str(&format!("{}Page()", pluralize(name)));
    s.push_str(" {\n  const rows = await ");
    s.push_str(name);
    s.push_str("Api.list({});\n  return (\n    <div>\n      <h1>");
    s.push_str(&title);
    s.push_str("</h1>\n      <p><Link href=\"/");
    s.push_str(&plural);
    s.push_str("/new\">+ 新建</Link></p>\n      <table border={1} cellPadding={8} style={{ borderCollapse: \"collapse\", width: \"100%\" }}>\n        <thead>\n          <tr>\n");
    for c in columns {
        let key = column_json_key(program, entity, c);
        s.push_str("            <th>");
        s.push_str(c);
        s.push_str("</th>\n");
        let _ = key;
    }
    s.push_str("            <th>操作</th>\n          </tr>\n        </thead>\n        <tbody>\n");
    s.push_str("          {rows.map((row) => (\n            <tr key={row.id}>\n");
    for c in columns {
        let key = column_json_key(program, entity, c);
        s.push_str("              <td>{String(row[\"");
        s.push_str(&key);
        s.push_str("\"] ?? \"\")}</td>\n");
    }
    s.push_str("              <td style={{ display: \"flex\", gap: 8 }}>\n");
    if api.operations.iter().any(|o| o == "update") {
        s.push_str("                <Link href={`/");
        s.push_str(&plural);
        s.push_str("/${row.id}`}>编辑</Link>\n");
    }
    for cmd in &cmds {
        let fn_name = to_snake_case(&cmd.name);
        let label = crate::dsl::pick_message(&cmd.label, &cmd.name);
        s.push_str("                <form action={async () => { \"use server\"; await ");
        s.push_str(&fn_name);
        s.push_str("(row.id); }} style={{ display: \"inline\" }}>\n");
        s.push_str("                  <button type=\"submit\">");
        s.push_str(&label);
        s.push_str("</button>\n                </form>\n");
    }
    if api.operations.iter().any(|o| o == "delete") {
        s.push_str("                <form action={async () => { \"use server\"; await delete");
        s.push_str(name);
        s.push_str("(row.id); }} style={{ display: \"inline\" }}>\n");
        s.push_str(
            "                  <button type=\"submit\">删除</button>\n                </form>\n",
        );
    }
    s.push_str("              </td>\n            </tr>\n          ))}\n        </tbody>\n      </table>\n    </div>\n  );\n}\n");
    s
}

/// 输入字段清单（Create/Update 共用；排除 @id/@readonly/createdAt 类）。
fn form_fields(program: &Program, entity: &crate::dsl::EntityDecl) -> Vec<(String, String, bool)> {
    // (snake, input_type, required)
    let mut out = Vec::new();
    for f in &entity.fields {
        if crate::dsl::has_attr(f, "id") || crate::dsl::has_attr(f, "readonly") {
            continue;
        }
        if f.type_name == "Uuid" && crate::dsl::has_attr(f, "id") {
            continue;
        }
        // Next v1 表单不支持关系选择（关系字段跳过，留后续 select 组件）——
        // 与 web-react 的关系字段行为对齐由后续迭代补齐。
        if is_relation(program, &f.type_name) {
            continue;
        }
        let snake = to_snake_case(&f.name);
        if snake == "created_at" || snake == "updated_at" {
            continue;
        }
        let required = !f.optional && crate::dsl::has_attr(f, "required");
        out.push((snake, input_type_of(&f.type_name).to_string(), required));
    }
    out
}

fn new_page_tsx(program: &Program, entity: &crate::dsl::EntityDecl) -> String {
    let _ = program;
    let name = &entity.name;
    let plural = to_snake_case(&pluralize(name));
    let fields = form_fields(program, entity);

    let mut s = String::new();
    s.push_str("\"use client\";\n\n");
    s.push_str("// @generated by Forma (gen-nextjs) — do not edit manually.\n");
    s.push_str("import React, { useState } from \"react\";\n");
    s.push_str("import { useRouter } from \"next/navigation\";\n");
    s.push_str("import { create");
    s.push_str(name);
    s.push_str(" } from \"../../../app/actions\";\n");
    s.push_str("import { validateCreate");
    s.push_str(name);
    s.push_str(" } from \"../../../src/validators\";\n\n");
    s.push_str("export default function New");
    s.push_str(name);
    s.push_str("Page() {\n");
    s.push_str("  const router = useRouter();\n");
    s.push_str("  const [errors, setErrors] = useState<string[]>([]);\n");
    s.push_str("  const [pending, setPending] = useState(false);\n");
    s.push_str("  return (\n    <div>\n      <h1>新建 ");
    s.push_str(name);
    s.push_str("</h1>\n");
    s.push_str("      {errors.length > 0 && (\n        <ul style={{ color: \"#e00\" }}>\n          {errors.map((e, i) => (<li key={i}>{e}</li>))}\n        </ul>\n      )}\n");
    s.push_str("      <form\n        action={async (formData: FormData) => {\n");
    s.push_str("          const input: Record<string, unknown> = {};\n");
    for (snake, itype, _req) in &fields {
        s.push_str("          input[\"");
        s.push_str(snake);
        s.push_str("\"] = formData.get(\"");
        s.push_str(snake);
        s.push_str("\");\n");
        if itype == "number" {
            s.push_str("          if (input[\"");
            s.push_str(snake);
            s.push_str("\"] === \"\") input[\"");
            s.push_str(snake);
            s.push_str("\"] = null;\n          else input[\"");
            s.push_str(snake);
            s.push_str("\"] = Number(input[\"");
            s.push_str(snake);
            s.push_str("\"]);\n");
        }
        let _ = _req;
    }
    s.push_str("          const errs = validateCreate");
    s.push_str(name);
    s.push_str("(input);\n          if (errs.length > 0) { setErrors(errs.map((e) => e.message)); return; }\n");
    s.push_str("          setPending(true);\n          await create");
    s.push_str(name);
    s.push_str("(input);\n          router.push(\"/");
    s.push_str(&plural);
    s.push_str("\");\n        }}\n      >\n");
    for (snake, itype, req) in &fields {
        s.push_str("        <div style={{ marginBottom: 12 }}>\n          <label style={{ display: \"block\", fontSize: 12, color: \"#666\" }}>");
        s.push_str(snake);
        if *req {
            s.push_str(" *");
        }
        s.push_str("</label>\n          <input name=\"");
        s.push_str(snake);
        s.push_str("\" type=\"");
        s.push_str(itype);
        s.push_str("\" style={{ width: 320, padding: 8 }} />\n        </div>\n");
    }
    s.push_str("        <button type=\"submit\" disabled={pending}>保存</button>\n      </form>\n    </div>\n  );\n}\n");
    s
}

fn edit_page_tsx(program: &Program, entity: &crate::dsl::EntityDecl) -> String {
    let _ = program;
    let name = &entity.name;
    let plural = to_snake_case(&pluralize(name));
    let fields = form_fields(program, entity);

    let mut s = String::new();
    s.push_str("\"use client\";\n\n");
    s.push_str("// @generated by Forma (gen-nextjs) — do not edit manually.\n");
    s.push_str("import React, { useEffect, useState } from \"react\";\n");
    s.push_str("import { useRouter } from \"next/navigation\";\n");
    s.push_str("import { get");
    s.push_str(name);
    s.push_str(", update");
    s.push_str(name);
    s.push_str(" } from \"../../../app/actions\";\n");
    s.push_str("import { validateUpdate");
    s.push_str(name);
    s.push_str(" } from \"../../../src/validators\";\n\n");
    s.push_str("export default function Edit");
    s.push_str(name);
    s.push_str("Page({ params }: { params: { id: string } }) {\n");
    s.push_str("  const router = useRouter();\n");
    s.push_str("  const [row, setRow] = useState<Record<string, unknown> | null>(null);\n");
    s.push_str("  const [errors, setErrors] = useState<string[]>([]);\n");
    s.push_str("  useEffect(() => {\n    get");
    s.push_str(name);
    s.push_str("(params.id).then((r) => setRow(r as unknown as Record<string, unknown>));\n  }, [params.id]);\n");
    s.push_str("  if (row === null) return <p>加载中…</p>;\n");
    s.push_str("  return (\n    <div>\n      <h1>编辑 ");
    s.push_str(name);
    s.push_str("</h1>\n");
    s.push_str("      {errors.length > 0 && (\n        <ul style={{ color: \"#e00\" }}>\n          {errors.map((e, i) => (<li key={i}>{e}</li>))}\n        </ul>\n      )}\n");
    s.push_str("      <form\n        action={async (formData: FormData) => {\n");
    s.push_str("          const input: Record<string, unknown> = {};\n");
    for (snake, itype, _req) in &fields {
        s.push_str("          input[\"");
        s.push_str(snake);
        s.push_str("\"] = formData.get(\"");
        s.push_str(snake);
        s.push_str("\");\n");
        if itype == "number" {
            s.push_str("          if (input[\"");
            s.push_str(snake);
            s.push_str("\"] === \"\") input[\"");
            s.push_str(snake);
            s.push_str("\"] = null;\n          else input[\"");
            s.push_str(snake);
            s.push_str("\"] = Number(input[\"");
            s.push_str(snake);
            s.push_str("\"]);\n");
        }
    }
    s.push_str("          const errs = validateUpdate");
    s.push_str(name);
    s.push_str("(input);\n          if (errs.length > 0) { setErrors(errs.map((e) => e.message)); return; }\n");
    s.push_str("          await update");
    s.push_str(name);
    s.push_str("(params.id, input);\n          router.push(\"/");
    s.push_str(&plural);
    s.push_str("\");\n        }}\n      >\n");
    for (snake, itype, req) in &fields {
        s.push_str("        <div style={{ marginBottom: 12 }}>\n          <label style={{ display: \"block\", fontSize: 12, color: \"#666\" }}>");
        s.push_str(snake);
        if *req {
            s.push_str(" *");
        }
        s.push_str("</label>\n          <input name=\"");
        s.push_str(snake);
        s.push_str("\" type=\"");
        s.push_str(itype);
        s.push_str("\" defaultValue={row[\"");
        s.push_str(snake);
        s.push_str("\"] === null ? \"\" : String(row[\"");
        s.push_str(snake);
        s.push_str("\"] ?? \"\")} style={{ width: 320, padding: 8 }} />\n        </div>\n");
    }
    s.push_str(
        "        <button type=\"submit\">保存</button>\n      </form>\n    </div>\n  );\n}\n",
    );
    s
}

fn ci_yaml() -> String {
    let mut s = String::new();
    s.push_str("name: web-ci\n\n");
    s.push_str("on:\n  push:\n    branches: [main]\n  pull_request:\n\n");
    s.push_str("jobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n");
    s.push_str("      - uses: actions/checkout@v4\n");
    s.push_str("      - uses: actions/setup-node@v4\n        with:\n          node-version: 20\n");
    s.push_str("      - name: Install\n        run: npm install --no-audit --no-fund\n");
    s.push_str("      - name: Typecheck\n        run: npx tsc --noEmit\n");
    s.push_str("      - name: Build\n        run: npm run build\n");
    s
}

fn readme(title: &str) -> String {
    let mut s = String::new();
    s.push_str("# ");
    s.push_str(title);
    s.push_str(" web (Next.js App Router, generated by Forma)\n\n```bash\nnpm install\nAPI_BASE=http://localhost:8080 npm run dev\n```\n\n");
    s.push_str("- 列表页为 Server Component（服务端直连后端，`API_BASE` env）；\n- 变更走 Server Actions（`app/actions.ts`）；\n- `next.config.mjs` 将 `/api/*` rewrite 到后端（客户端相对路径可用）；\n- 表单校验复用 `src/validators.ts`（与 Rust 后端同源）；\n- v1 范围：CRUD + 空 input 命令按钮；auth UI 与带 input 命令对话框为后续增强。\n");
    s
}
