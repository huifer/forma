//! Vue 3 frontend target（M10）：Vue Router + Pinia + Element Plus。

use crate::dsl::{pick_message, PageBody, Program};
use crate::generators::GeneratedFile;
use crate::identity::{to_camel_case, to_snake_case};
use std::path::Path;

pub fn emit_vue(
    project_dir: &Path,
    program: &Program,
    out_dir: &str,
    app_name: &str,
) -> anyhow::Result<Vec<GeneratedFile>> {
    if program.entities.is_empty() || program.pages.is_empty() {
        return Ok(vec![]);
    }
    let mut files = vec![
        file(out_dir, "package.json", package_json(app_name)),
        file(out_dir, "vite.config.ts", vite_config()),
        file(out_dir, "tsconfig.json", tsconfig()),
        file(out_dir, "src/env.d.ts", env_d_ts()),
        file(out_dir, "src/App.vue", app_vue()),
        file(out_dir, "index.html", index_html(app_name)),
        file(out_dir, "src/main.ts", main_ts()),
        file(out_dir, "src/api.ts", api_ts(program)),
        file(
            out_dir,
            "src/types.ts",
            crate::emit::web_react::types_ts(program),
        ),
        file(
            out_dir,
            "src/validators.ts",
            crate::emit::web_react::validators_ts(program),
        ),
        file(
            out_dir,
            "src/format.ts",
            crate::emit::web_react::format_ts(program),
        ),
    ];
    if !crate::emit::web_react::frontend_access_rules(program).is_empty()
        || !crate::dsl::field_policy_roles(program).is_empty()
    {
        files.push(file(out_dir, "src/access.ts", access_vue_ts(program)));
    }
    files.push(file(out_dir, "src/router.ts", router_ts(program)));
    for page in &program.pages {
        let name = to_snake_case(&page.name);
        files.push(file(
            out_dir,
            &format!("src/pages/{name}.vue"),
            page_vue(program, page),
        ));
    }
    files.push(file(out_dir, "README.md", readme(app_name)));

    for f in &files {
        crate::util::write_text(&project_dir.join(&f.path), &f.content)?;
    }
    Ok(files)
}

fn file(out: &str, rel: &str, content: String) -> GeneratedFile {
    GeneratedFile {
        path: format!("{out}/{rel}"),
        content,
        generator: "gen-vue".into(),
    }
}

fn package_json(app: &str) -> String {
    let pkg = to_snake_case(app).replace('_', "-");
    let s = format!("{{ \"name\": \"{pkg}-web\", \"private\": true, \"version\": \"0.1.0\", \"type\": \"module\", \"scripts\": {{ \"dev\": \"vite\", \"build\": \"vue-tsc --noEmit && vite build\", \"test\": \"vitest run\" }}, \"dependencies\": {{ \"@element-plus/icons-vue\": \"^2.3.1\", \"@tanstack/vue-query\": \"^5.59.0\", \"element-plus\": \"^2.8.0\", \"pinia\": \"^2.2.0\", \"vue\": \"^3.5.0\", \"vue-router\": \"^4.4.0\" }}, \"devDependencies\": {{ \"@types/node\": \"^22.0.0\", \"@vitejs/plugin-vue\": \"^5.1.0\", \"typescript\": \"^5.5.4\", \"vite\": \"^5.4.0\", \"vitest\": \"^2.1.9\", \"vue-tsc\": \"^2.1.0\" }} }}");
    s
}

fn vite_config() -> String {
    "import { defineConfig } from \"vite\";\nimport vue from \"@vitejs/plugin-vue\";\nexport default defineConfig({ plugins: [vue()] });\n".into()
}

fn tsconfig() -> String {
    "{\n  \"compilerOptions\": {\n    \"target\": \"ES2020\", \"module\": \"ESNext\", \"moduleResolution\": \"Bundler\",\n    \"strict\": true, \"jsx\": \"preserve\", \"resolveJsonModule\": true,\n    \"esModuleInterop\": true, \"skipLibCheck\": true, \"noEmit\": true\n  },\n  \"include\": [\"src/**/*.ts\", \"src/**/*.vue\"]\n}\n".into()
}

fn env_d_ts() -> String {
    "/// <reference types=\"vite/client\" />\n".into()
}

fn app_vue() -> String {
    "<template><router-view /></template>\n".into()
}

fn index_html(app: &str) -> String {
    format!("<!doctype html>\n<html>\n<head><meta charset=\"UTF-8\" /><meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\" /><title>{app}</title></head>\n<body><div id=\"app\"></div><script type=\"module\" src=\"/src/main.ts\"></script></body></html>\n")
}

fn main_ts() -> String {
    "import { createApp } from \"vue\";\nimport { createPinia } from \"pinia\";\nimport { VueQueryPlugin } from \"@tanstack/vue-query\";\nimport ElementPlus from \"element-plus\";\nimport \"element-plus/dist/index.css\";\nimport App from \"./App.vue\";\nimport router from \"./router\";\n\ncreateApp(App).use(createPinia()).use(router).use(VueQueryPlugin).use(ElementPlus).mount(\"#app\");\n".into()
}

fn api_ts(program: &Program) -> String {
    let mut out = String::new();
    out.push_str("export const API_BASE = import.meta.env.VITE_API_BASE ?? \"/api\";\n\n");
    out.push_str("export interface FieldError { field: string; message: string; }\n\n");
    let mut type_names = String::new();
    for e in &program.entities {
        type_names.push_str(&format!(
            "{}, {}ListRow, Create{}, Update{}, ",
            e.name, e.name, e.name, e.name
        ));
    }
    out.push_str(&format!(
        "import {{ {} }} from \"./types\";\n\n",
        type_names.trim_end_matches(", ")
    ));
    out.push_str("async function req<T>(path: string, init?: RequestInit): Promise<T> {\n");
    out.push_str("  const res = await fetch(`${API_BASE}${path}`, { ...init, headers: { \"Content-Type\": \"application/json\", ...(init?.headers ?? {}) } });\n");
    out.push_str("  const body = await res.json().catch(() => ({}));\n");
    out.push_str(
        "  if (!res.ok) throw Object.assign(new Error(body.error ?? res.statusText), body);\n",
    );
    out.push_str("  return body as T;\n}\n\n");
    for entity in &program.entities {
        let name = &entity.name;
        let plural = crate::dsl::pluralize(name).to_lowercase();
        let api_prefix = format!(
            "export const {}Api = {{\n  list: (query: Record<string, unknown> = {{}}) => req<{}ListRow[]>(`/{}?${{new URLSearchParams(query as Record<string, string>)}}`),\n  get: (id: string) => req<{}>(`/{}/${{id}}`),\n  create: (body: Create{}) => req<{}>(\"/{}/\", {{ method: \"POST\", body: JSON.stringify(body) }}),\n  update: (id: string, body: Update{}) => req<{}>(`/{}/${{id}}`, {{ method: \"PUT\", body: JSON.stringify(body) }}),\n",
            name, name, plural, name, plural, name, name, plural, name, name, plural
        );
        out.push_str(&api_prefix);
        let remove_line = format!(
            "  remove: (id: string) => req<{{ ok: boolean }}>(`/{plural}/${{id}}`, {{ method: \"DELETE\" }}),\n}};\n\n",
            plural = plural,
        );
        out.push_str(&remove_line);
    }
    out.push_str(&crate::emit::web_react::auth_api_ts(program));
    out.push_str(&crate::emit::web_react::commands_api_ts(program));
    out
}

fn router_ts(program: &Program) -> String {
    let mut routes = String::new();
    for p in &program.pages {
        routes.push_str(&format!(
            "  {{ path: \"{}\", component: () => import(\"./pages/{}.vue\") }},\n",
            p.route,
            to_snake_case(&p.name)
        ));
    }
    format!(
        "import {{ createRouter, createWebHistory }} from \"vue-router\";\nexport default createRouter({{ history: createWebHistory(), routes: [\n{routes}] }});\n"
    )
}

fn access_vue_ts(program: &Program) -> String {
    let rules = crate::emit::web_react::frontend_access_rules(program);
    let field_rules = crate::dsl::field_policy_roles(program);
    let mut entries = String::new();
    for (entity, ops, roles) in &rules {
        let ops = ops
            .iter()
            .map(|o| format!("{:?}", o))
            .collect::<Vec<_>>()
            .join(", ");
        let roles = roles
            .iter()
            .map(|r| format!("{:?}", r))
            .collect::<Vec<_>>()
            .join(", ");
        entries.push_str(&format!(
            "  {:?}: {{ ops: [{}], roles: [{}] }},\n",
            entity, ops, roles
        ));
    }
    let mut field_entries = String::new();
    let mut by_entity: std::collections::BTreeMap<String, Vec<(String, Vec<String>)>> =
        Default::default();
    for (entity, field, roles) in &field_rules {
        by_entity
            .entry(entity.clone())
            .or_default()
            .push((field.clone(), roles.clone()));
    }
    for (entity, fields) in &by_entity {
        let mut inner = String::new();
        for (field, roles) in fields {
            let roles = roles
                .iter()
                .map(|r| format!("{:?}", r))
                .collect::<Vec<_>>()
                .join(", ");
            inner.push_str(&format!("      {:?}: [{}],\n", field, roles));
        }
        field_entries.push_str(&format!("  {:?}: {{\n{inner}  }},\n", entity));
    }
    let mut out = String::from("// @generated by Forma (gen-vue) — do not edit manually.\n\n");
    if program.auth.is_some() {
        out.push_str(
            "import { computed } from \"vue\";\nimport { useQuery } from \"@tanstack/vue-query\";\nimport { AuthApi } from \"./api\";\n\n",
        );
    }
    out.push_str(
        "export const ACCESS_RULES: Record<string, { ops: string[]; roles: string[] }> = {\n",
    );
    out.push_str(&entries);
    out.push_str("};\n\n");
    out.push_str("export const FIELD_RULES: Record<string, Record<string, string[]>> = {\n");
    out.push_str(&field_entries);
    out.push_str("};\n\n");
    out.push_str(
        "/** 某字段对当前角色是否可见（体验层；role = null 表示角色未知 → 一律可见）。 */\n",
    );
    out.push_str(
        "export function canSeeField(entity: string, field: string, role: string | null): boolean {\n",
    );
    out.push_str("  const fields = FIELD_RULES[entity];\n");
    out.push_str("  if (!fields || !fields[field]) return true;\n");
    out.push_str("  if (!role) return true;\n");
    out.push_str("  return fields[field].length === 0 || fields[field].includes(role);\n}\n\n");
    if program.auth.is_some() {
        out.push_str("/** 当前登录用户的角色；未登录或加载中为 null。 */\n");
        out.push_str(
            "export function useCurrentRole(): import(\"vue\").ComputedRef<string | null> {\n",
        );
        out.push_str("  const { data } = useQuery({ queryKey: [\"auth-me\"], queryFn: () => AuthApi.me(), retry: false });\n");
        out.push_str("  return computed(() => data.value?.role ?? null);\n}\n\n");
        out.push_str("/** 能否执行某实体的某操作（体验层；角色未知时不隐藏）。 */\n");
        out.push_str(
            "export function useCan(entity: string, op: string): import(\"vue\").ComputedRef<boolean> {\n",
        );
        out.push_str("  const role = useCurrentRole();\n");
        out.push_str("  return computed(() => {\n");
        out.push_str("    const r = role.value;\n");
        out.push_str("    const rule = ACCESS_RULES[entity];\n");
        out.push_str("    if (!rule || !rule.ops.includes(op)) return true;\n");
        out.push_str("    if (!r) return true;\n");
        out.push_str("    return rule.roles.length === 0 || rule.roles.includes(r);\n  });\n}\n");
    }
    out
}

fn page_vue(program: &Program, page: &crate::dsl::PageDecl) -> String {
    match &page.body {
        PageBody::Table(cfg) => table_vue(program, page, cfg),
        PageBody::Form(cfg) => form_vue(program, page, cfg),
        PageBody::Detail(cfg) => detail_vue(program, page, cfg),
        PageBody::Chart(_) => simple_vue(page, "chart"),
    }
}

fn table_vue(program: &Program, page: &crate::dsl::PageDecl, cfg: &crate::dsl::TableCfg) -> String {
    let Some(api) = program.apis.iter().find(|a| a.name == cfg.query_api) else {
        return simple_vue(page, "unknown api");
    };
    let Some(entity) = program.entities.iter().find(|e| e.name == api.entity) else {
        return simple_vue(page, "unknown entity");
    };
    let title = pick_message(&page.title, &page.name);
    let api_name = entity.name.clone();
    let mut headers = String::new();
    for c in &cfg.columns {
        let field = c.path.rsplit('.').next().unwrap_or(&c.path);
        headers.push_str(&format!(
            "    <el-table-column prop=\"{field}\" label=\"{label}\" />\n",
            label = field
        ));
    }
    format!(
        "<script setup lang=\"ts\">\nimport {{ ref, onMounted }} from \"vue\";\nimport {{ {api}Api }} from \"../api\";\nconst rows = ref<any[]>([]);\nconst loading = ref(false);\nasync function reload() {{\n  loading.value = true;\n  rows.value = await {api}Api.list();\n  loading.value = false;\n}}\nonMounted(reload);\n</script>\n\n<template>\n  <div class=\"page\">\n    <h1>{title}</h1>\n    <el-button @click=\"reload\">刷新</el-button>\n    <el-table :data=\"rows\" v-loading=\"loading\" stripe>\n{headers}    </el-table>\n  </div>\n</template>\n",
        api = api_name,
        title = title,
        headers = headers
    )
}

fn form_vue(_program: &Program, page: &crate::dsl::PageDecl, cfg: &crate::dsl::FormCfg) -> String {
    let title = pick_message(&page.title, &page.name);
    let fields = cfg
        .fields
        .iter()
        .map(|f| {
            let key = to_camel_case(&to_snake_case(&f.path));
            format!(
                "    <el-form-item label=\"{label}\"><el-input v-model=\"form.{key}\" /></el-form-item>\n",
                label = f.path
            )
        })
        .collect::<String>();
    format!(
        "<script setup lang=\"ts\">\nimport {{ reactive }} from \"vue\";\nconst form = reactive<Record<string, unknown>>({{}});\n</script>\n\n<template>\n  <div class=\"page\"><h1>{title}</h1>\n    <el-form label-width=\"120px\">\n{fields}      <el-button type=\"primary\">保存</el-button>\n    </el-form>\n  </div>\n</template>\n",
        title = title,
        fields = fields
    )
}

fn detail_vue(
    _program: &Program,
    page: &crate::dsl::PageDecl,
    _cfg: &crate::dsl::DetailCfg,
) -> String {
    let title = pick_message(&page.title, &page.name);
    format!(
        "<template><div class=\"page\"><h1>{title}</h1><p>详情</p></div></template>\n",
        title = title
    )
}

fn simple_vue(page: &crate::dsl::PageDecl, body: &str) -> String {
    let title = pick_message(&page.title, &page.name);
    format!(
        "<template><div class=\"page\"><h1>{title}</h1><p>{body}</p></div></template>\n",
        title = title,
        body = body
    )
}

fn readme(app: &str) -> String {
    format!("# {app} Vue frontend (generated by Forma)\n\n```bash\nnpm install\nnpm run dev\n```\n")
}
