//! Vue 3 frontend target（M10）：Vue Router + Pinia + Element Plus。

use crate::dsl::{pick_message, PageBody, Program};
use crate::generators::GeneratedFile;
use crate::identity::{to_pascal_case, to_snake_case};
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
    // 宪法 6.2：看板（dashboard）页暂仅 Web 支持
    if program
        .pages
        .iter()
        .any(|p| matches!(p.body, crate::dsl::PageBody::Dashboard(_)))
    {
        anyhow::bail!(
            "Vue target does not support dashboard pages yet (web-only in React adapter)"
        );
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
        if matches!(page.body, PageBody::Table(_)) {
            files.push(file(
                out_dir,
                &format!("src/stores/{name}.ts"),
                store_ts(&page.name),
            ));
        }
        files.push(file(
            out_dir,
            &format!("src/pages/{name}.vue"),
            page_vue(program, page),
        ));
    }
    files.push(file(
        out_dir,
        "README.md",
        format!(
            "{}{}",
            crate::emit::generated_md_header("gen-vue"),
            readme(app_name)
        ),
    ));

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
    // api 块名别名（页面 submit/query 以块名引用，与 web_react 契约一致）
    for api in &program.apis {
        let entity_api = format!("{}Api", api.entity);
        out.push_str(&format!(
            "export const {}Api = {};// alias\n",
            api.name, entity_api
        ));
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
        // 不可达：emit_vue 已在生成前拒绝 dashboard（见函数头）
        PageBody::Dashboard(_) => String::new(),
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
    let entity_name = entity.name.clone();
    let plural = to_snake_case(&crate::dsl::pluralize(&entity_name));
    let snake_e = to_snake_case(&entity_name);
    let store_id = to_snake_case(&page.name);
    let store_camel = to_pascal_case(&store_id);

    // ── 过滤器：关系/枚举 → select（options 查询）；其余 → input ──
    #[derive(Clone)]
    enum FilterKind {
        Input,
        Enum(String),
        Relation(String),
    }
    let mut filters: Vec<(String, String, FilterKind)> = Vec::new();
    for f in &cfg.filters {
        let field = entity.fields.iter().find(|x| x.name == f.path);
        let kind = match field {
            Some(f) if crate::dsl::is_relation(program, &f.type_name) => {
                FilterKind::Relation(f.type_name.clone())
            }
            Some(f) if crate::dsl::is_enum(program, &f.type_name) => {
                FilterKind::Enum(f.type_name.clone())
            }
            _ => FilterKind::Input,
        };
        filters.push((to_snake_case(&f.path), f.path.clone(), kind));
    }

    let mut api_imports: Vec<String> = vec![format!("{}Api", api.name)];
    let mut option_queries = String::new();
    let mut seen_sources: Vec<String> = Vec::new();
    for (_, _, kind) in &filters {
        if let FilterKind::Relation(src) = kind {
            if !seen_sources.contains(src) {
                seen_sources.push(src.clone());
                api_imports.push(format!("{}Api", src));
                let lower = to_snake_case(src);
                option_queries.push_str(&format!(
                    "const {src}Options = useQuery({{ queryKey: [\"{lower}-options\"], queryFn: () => {src}Api.list({{ pageSize: 200 }}) }});\n"
                ));
            }
        }
    }
    let has_enum_filter = filters
        .iter()
        .any(|(_, _, k)| matches!(k, FilterKind::Enum(_)));

    // ── 命令按钮（空 input）──
    let mut cmd_mutations = String::new();
    let mut cmd_buttons = String::new();
    for cmd in program.commands.iter().filter(|c| {
        c.input.is_empty()
            && c.effects.first().map(|e| e.entity.clone()) == Some(entity_name.clone())
    }) {
        let fn_name = to_snake_case(&cmd.name);
        let label = pick_message(&cmd.label, &cmd.name);
        let imp = format!("{}Api", cmd.name);
        if !api_imports.contains(&imp) {
            api_imports.push(imp);
        }
        cmd_mutations.push_str(&format!(
            "const {fn_name}Mut = useMutation({{ mutationFn: (id: string) => {}Api.execute(id, {{}}), onSuccess: () => qc.invalidateQueries({{ queryKey: [\"{snake_e}\"] }}) }});\n",
            cmd.name
        ));
        cmd_buttons.push_str(&format!(
            "            <el-button link @click=\"{fn_name}Mut.mutate(row.id)\">{{{{ {label} }}}}</el-button>\n"
        ));
    }

    let has_create = api.operations.iter().any(|o| o == "create");
    let has_update = api.operations.iter().any(|o| o == "update");
    let has_delete = api.operations.iter().any(|o| o == "delete");
    let export_csv = program
        .exports
        .iter()
        .any(|e| e.api == api.name && e.format == "csv");

    // 授权感知（体验层）：存在授权规则才导入 useCan
    let gated = crate::emit::web_react::frontend_access_rules(program)
        .iter()
        .any(|(e, _, _)| e == &entity_name);
    let access_import = if gated {
        "import { useCan } from \"../access\";\n".to_string()
    } else {
        String::new()
    };
    let can_hooks = if gated {
        format!(
            "const canCreate = useCan(\"{entity_name}\", \"create\");\nconst canUpdate = useCan(\"{entity_name}\", \"update\");\nconst canDelete = useCan(\"{entity_name}\", \"delete\");\n"
        )
    } else {
        String::new()
    };

    let mut filter_code = String::new();
    for (snake, label, kind) in &filters {
        match kind {
            FilterKind::Input => {
                filter_code.push_str(&format!(
                    "      <el-input v-model=\"store.filters.{snake}\" placeholder=\"{label}\" clearable @input=\"store.resetPage()\" />\n"
                ));
            }
            FilterKind::Enum(name) => {
                filter_code.push_str(&format!(
                    "      <el-select v-model=\"store.filters.{snake}\" placeholder=\"{label}\" clearable @change=\"store.resetPage()\">\n        <el-option v-for=\"o in enumOptions('{name}')\" :key=\"o.value\" :value=\"o.value\" :label=\"o.label\" />\n      </el-select>\n"
                ));
            }
            FilterKind::Relation(src) => {
                let display = crate::emit::web_react::display_field_of(program, src);
                filter_code.push_str(&format!(
                    "      <el-select v-model=\"store.filters.{snake}\" placeholder=\"{label}\" clearable @change=\"store.resetPage()\">\n        <el-option v-for=\"o in ({src}Options.data.value ?? [])\" :key=\"o.id\" :value=\"o.id\" :label=\"o.{display}\" />\n      </el-select>\n"
                ));
            }
        }
    }

    let mut headers = String::new();
    for c in &cfg.columns {
        let field = c.path.rsplit('.').next().unwrap_or(&c.path);
        // 关系列显示 `{rel}_name`（ListRow 契约）
        let is_rel = entity
            .fields
            .iter()
            .find(|x| x.name == c.path)
            .map(|x| crate::dsl::is_relation(program, &x.type_name))
            .unwrap_or(false);
        let prop = if is_rel {
            format!("{}_name", to_snake_case(&c.path))
        } else {
            to_snake_case(&c.path)
        };
        headers.push_str(&format!(
            "      <el-table-column prop=\"{prop}\" label=\"{field}\" />\n"
        ));
    }

    let nav_fns = format!(
        "function goNew() {{ router.push(\"/{plural}/new\"); }}\nfunction go(id: string) {{ router.push(`/{plural}/${{id}}`); }}\n"
    );
    let create_btn = if has_create {
        "    <el-button type=\"primary\" @click=\"goNew\">新建</el-button>\n".to_string()
    } else {
        String::new()
    };
    let edit_btn = if has_update {
        "            <el-button link @click=\"go(row.id)\">编辑</el-button>\n".to_string()
    } else {
        String::new()
    };
    let delete_btn = if has_delete {
        "            <el-button link type=\"danger\" @click=\"deleteMut.mutate(row.id)\">删除</el-button>\n".to_string()
    } else {
        String::new()
    };
    let delete_mut = if has_delete {
        format!(
            "const deleteMut = useMutation({{ mutationFn: (id: string) => {api_name}Api.remove(id), onSuccess: () => qc.invalidateQueries({{ queryKey: [\"{snake_e}\"] }}) }});\n",
            api_name = api.name
        )
    } else {
        String::new()
    };
    let export_fn = if export_csv {
        format!(
            "function exportCsv() {{ fetch(`/api/{plural}/export.csv`).then((r) => r.blob()).then((b) => {{ const url = URL.createObjectURL(b); const a = document.createElement(\"a\"); a.href = url; a.download = \"{plural}.csv\"; a.click(); URL.revokeObjectURL(url); }}); }}\n"
        )
    } else {
        String::new()
    };
    let export_btn = if export_csv {
        "    <el-button @click=\"exportCsv\">导出 CSV</el-button>\n".to_string()
    } else {
        String::new()
    };

    let template = r#"<script setup lang="ts">
import { computed } from "vue";
import { useRouter } from "vue-router";
import { useQuery, useMutation, useQueryClient } from "@tanstack/vue-query";
import { %%API_IMPORTS%% } from "../api";
import { use%%STORE_CAMEL%%Store } from "../stores/%%STORE_ID%%";
%%ACCESS_IMPORT%%%%ENUM_IMPORT%%const store = use%%STORE_CAMEL%%Store();
const router = useRouter();
const qc = useQueryClient();
const query = useQuery({
  queryKey: computed(() => ["%%SNAKE_E%%", store.filters, store.search, store.page]),
  queryFn: () => %%API_NAME%%Api.list({ ...store.filters, q: store.search || null, page: store.page }),
});
const rows = computed(() => query.data.value ?? []);
%%NAV_FNS%%%%DELETE_MUT%%%%CMD_MUTATIONS%%%%OPTION_QUERIES%%%%EXPORT_FN%%%%CAN_HOOKS%%</script>

<template>
  <div class="page">
    <h1>%%TITLE%%</h1>
%%CREATE_BTN%%    <div class="filters">
      <el-input v-model="store.search" placeholder="搜索…" clearable @input="store.resetPage()" />
%%FILTER_CODE%%    </div>
    <el-table :data="rows" v-loading="query.isLoading.value" stripe>
%%HEADERS%%      <el-table-column label="操作">
        <template #default="{ row }">
%%EDIT_BTN%%%%DELETE_BTN%%%%CMD_BUTTONS%%        </template>
      </el-table-column>
    </el-table>
    <div class="pager">
      <el-button :disabled="store.page <= 1" @click="store.prev()">上一页</el-button>
      <span>第 {{ store.page }} 页</span>
      <el-button :disabled="rows.length < 20" @click="store.next()">下一页</el-button>
    </div>
%%EXPORT_BTN%%  </div>
</template>
"#;
    template
        .replace("%%API_IMPORTS%%", &api_imports.join(", "))
        .replace("%%STORE_CAMEL%%", &store_camel)
        .replace("%%STORE_ID%%", &store_id)
        .replace("%%ACCESS_IMPORT%%", &access_import)
        .replace(
            "%%ENUM_IMPORT%%",
            if has_enum_filter {
                "import { enumOptions } from \"../format\";\n"
            } else {
                ""
            },
        )
        .replace("%%SNAKE_E%%", &snake_e)
        .replace("%%API_NAME%%", &api.name)
        .replace("%%NAV_FNS%%", &nav_fns)
        .replace("%%DELETE_MUT%%", &delete_mut)
        .replace("%%CMD_MUTATIONS%%", &cmd_mutations)
        .replace("%%OPTION_QUERIES%%", &option_queries)
        .replace("%%EXPORT_FN%%", &export_fn)
        .replace("%%CAN_HOOKS%%", &can_hooks)
        .replace("%%TITLE%%", &escape_html(&title))
        .replace("%%CREATE_BTN%%", &create_btn)
        .replace("%%FILTER_CODE%%", &filter_code)
        .replace("%%HEADERS%%", &headers)
        .replace("%%EDIT_BTN%%", &edit_btn)
        .replace("%%DELETE_BTN%%", &delete_btn)
        .replace("%%CMD_BUTTONS%%", &cmd_buttons)
        .replace("%%EXPORT_BTN%%", &export_btn)
}

/// Pinia store（列表页数据依赖：filters / search / 分页）。
fn store_ts(page_name: &str) -> String {
    let id = to_snake_case(page_name);
    let camel = to_pascal_case(&id);
    format!(
        "// @generated by Forma (gen-vue) — do not edit manually.\nimport {{ defineStore }} from \"pinia\";\nimport {{ ref }} from \"vue\";\n\nexport const use{camel}Store = defineStore(\"{id}\", () => {{\n  const filters = ref<Record<string, string | null>>({{}});\n  const search = ref(\"\");\n  const page = ref(1);\n  function setFilter(key: string, value: string | null) {{\n    filters.value = {{ ...filters.value, [key]: value }};\n    page.value = 1;\n  }}\n  function resetPage() {{ page.value = 1; }}\n  function next() {{ page.value += 1; }}\n  function prev() {{ page.value = Math.max(1, page.value - 1); }}\n  return {{ filters, search, page, setFilter, resetPage, next, prev }};\n}});\n"
    )
}

fn form_vue(program: &Program, page: &crate::dsl::PageDecl, cfg: &crate::dsl::FormCfg) -> String {
    let Some(entity) = program.entities.iter().find(|e| e.name == cfg.entity) else {
        return simple_vue(page, "unknown entity");
    };
    let title = pick_message(&page.title, &page.name);
    let entity_name = entity.name.clone();
    let plural = to_snake_case(&crate::dsl::pluralize(&entity_name));
    let redirect = cfg
        .submits
        .first()
        .map(|s| s.redirect.clone())
        .unwrap_or_else(|| format!("/{plural}"));

    #[derive(Clone)]
    enum FieldKind {
        Input,
        Number,
        Textarea,
        Enum(String),
        Relation(String),
    }
    struct Ff {
        snake: String,
        label: String,
        required: bool,
        kind: FieldKind,
    }
    let mut fields: Vec<Ff> = Vec::new();
    for f in &cfg.fields {
        let field = entity.fields.iter().find(|x| x.name == f.path);
        let Some(field) = field else { continue };
        if crate::dsl::has_attr(field, "id") || crate::dsl::has_attr(field, "readonly") {
            continue;
        }
        let snake = to_snake_case(&f.path);
        let required = f.required || !field.optional;
        let kind = if crate::dsl::is_relation(program, &field.type_name) {
            FieldKind::Relation(field.type_name.clone())
        } else if crate::dsl::is_enum(program, &field.type_name) {
            FieldKind::Enum(field.type_name.clone())
        } else if f.widget == "Textarea" {
            FieldKind::Textarea
        } else if matches!(field.type_name.as_str(), "Int" | "Float" | "Decimal") {
            FieldKind::Number
        } else {
            FieldKind::Input
        };
        fields.push(Ff {
            snake,
            label: f.path.clone(),
            required,
            kind,
        });
    }

    let mut api_imports: Vec<String> = vec![format!("{}Api", entity_name)];
    let mut option_queries = String::new();
    let mut rel_map = String::new();
    let mut seen_sources: Vec<String> = Vec::new();
    let mut has_enum = false;
    for f in &fields {
        match &f.kind {
            FieldKind::Relation(src) => {
                if !seen_sources.contains(src) {
                    seen_sources.push(src.clone());
                    api_imports.push(format!("{}Api", src));
                    let lower = to_snake_case(src);
                    option_queries.push_str(&format!(
                        "const {src}Options = useQuery({{ queryKey: [\"{lower}-options\"], queryFn: () => {src}Api.list({{ pageSize: 200 }}) }});\n"
                    ));
                }
                rel_map.push_str(&format!(
                    "  if (row[\"{snake}_id\"] != null) form.{snake} = row[\"{snake}_id\"];\n",
                    snake = f.snake
                ));
            }
            FieldKind::Enum(_) => has_enum = true,
            _ => {}
        }
    }

    let mut field_items = String::new();
    for f in &fields {
        let req = if f.required { " *" } else { "" };
        let snake = &f.snake;
        let input = match &f.kind {
            FieldKind::Number => format!(
                "<el-input v-model=\"form.{snake}\" type=\"number\" />"
            ),
            FieldKind::Textarea =>
                format!("<el-input v-model=\"form.{snake}\" type=\"textarea\" />"),
            FieldKind::Enum(name) => format!(
                "<el-select v-model=\"form.{snake}\" clearable><el-option v-for=\"o in enumOptions('{name}')\" :key=\"o.value\" :value=\"o.value\" :label=\"o.label\" /></el-select>"
            ),
            FieldKind::Relation(src) => {
                let display = crate::emit::web_react::display_field_of(program, src);
                format!(
                    "<el-select v-model=\"form.{snake}\" clearable><el-option v-for=\"o in ({src}Options.data.value ?? [])\" :key=\"o.id\" :value=\"o.id\" :label=\"o.{display}\" /></el-select>"
                )
            }
            FieldKind::Input => format!("<el-input v-model=\"form.{snake}\" />"),
        };
        field_items.push_str(&format!(
            "      <el-form-item label=\"{label}{req}\" :error=\"errors.{snake}\">{input}</el-form-item>\n",
            label = f.label,
            req = req,
            snake = f.snake,
            input = input
        ));
    }

    let template = r#"<script setup lang="ts">
import { reactive, ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import { useQuery } from "@tanstack/vue-query";
import { %%API_IMPORTS%% } from "../api";
import { validateCreate%%ENTITY%%, validateUpdate%%ENTITY%% } from "../validators";
%%ENUM_IMPORT%%const route = useRoute();
const router = useRouter();
const rawId = route.params.id;
const id = Array.isArray(rawId) ? rawId[0] : rawId;
const isCreate = !id || id === "new";
const form = reactive<Record<string, unknown>>({});
const errors = ref<Record<string, string>>({});
const pending = ref(false);
%%OPTION_QUERIES%%async function load() {
  if (isCreate) return;
  const row = (await %%ENTITY%%Api.get(id!)) as Record<string, any>;
  Object.assign(form, row);
  // 关系字段：实体响应 `{rel}_id` → 输入体裸名（后端 serde 两套键名体系）
%%REL_MAP%%}
void load();
async function submit() {
  const errs = isCreate ? validateCreate%%ENTITY%%(form) : validateUpdate%%ENTITY%%(form);
  if (errs.length > 0) {
    const m: Record<string, string> = {};
    for (const e of errs) m[e.field] = e.message;
    errors.value = m;
    return;
  }
  errors.value = {};
  pending.value = true;
  try {
    if (isCreate) await %%ENTITY%%Api.create(form as any);
    else await %%ENTITY%%Api.update(id!, form as any);
    router.push("%%REDIRECT%%");
  } catch (e: any) {
    const m: Record<string, string> = {};
    for (const f of e?.fields ?? []) m[f.field] = f.message;
    errors.value = m;
  } finally {
    pending.value = false;
  }
}
</script>

<template>
  <div class="page">
    <h1>%%TITLE%%</h1>
    <el-form label-width="120px">
%%FIELD_ITEMS%%      <el-form-item>
        <el-button type="primary" :loading="pending" @click="submit">保存</el-button>
      </el-form-item>
    </el-form>
  </div>
</template>
"#;
    template
        .replace("%%API_IMPORTS%%", &api_imports.join(", "))
        .replace("%%ENTITY%%", &entity_name)
        .replace(
            "%%ENUM_IMPORT%%",
            if has_enum {
                "import { enumOptions } from \"../format\";\n"
            } else {
                ""
            },
        )
        .replace("%%OPTION_QUERIES%%", &option_queries)
        .replace("%%REL_MAP%%", &rel_map)
        .replace("%%REDIRECT%%", &redirect)
        .replace("%%TITLE%%", &escape_html(&title))
        .replace("%%FIELD_ITEMS%%", &field_items)
}

fn detail_vue(
    program: &Program,
    page: &crate::dsl::PageDecl,
    cfg: &crate::dsl::DetailCfg,
) -> String {
    let Some(entity) = program.entities.iter().find(|e| e.name == cfg.entity) else {
        return simple_vue(page, "unknown entity");
    };
    let title = pick_message(&page.title, &page.name);
    let entity_name = entity.name.clone();
    let snake_e = to_snake_case(&entity_name);

    let mut rows = String::new();
    for (section, fields) in &cfg.sections {
        for (path, hint) in fields {
            let field = entity.fields.iter().find(|x| x.name == *path);
            let is_rel = field
                .map(|x| crate::dsl::is_relation(program, &x.type_name))
                .unwrap_or(false);
            let key = if is_rel {
                format!("{}_name", to_snake_case(path))
            } else {
                to_snake_case(path)
            };
            let value = match hint.as_deref() {
                Some("money") => format!("{{{{ fmtMoney(row.{key}) }}}}"),
                Some("datetime") => format!("{{{{ fmtDateTime(row.{key}) }}}}"),
                _ => format!("{{{{ row.{key} }}}}"),
            };
            rows.push_str(&format!(
                "        <el-descriptions-item label=\"{label}\">{value}</el-descriptions-item>\n",
                label = path,
                value = value
            ));
        }
        let _ = section;
    }

    let template = r#"<script setup lang="ts">
import { computed } from "vue";
import { useRoute } from "vue-router";
import { useQuery } from "@tanstack/vue-query";
import { %%ENTITY%%Api } from "../api";
%%FMT_IMPORT%%const route = useRoute();
const id = String(route.params.id ?? "");
const query = useQuery({ queryKey: computed(() => ["%%SNAKE_E%%", id]), queryFn: () => %%ENTITY%%Api.get(id) });
const row = computed(() => query.data.value);
</script>

<template>
  <div class="page">
    <h1>%%TITLE%%</h1>
    <el-descriptions v-if="row" :column="1" border>
%%ROWS%%    </el-descriptions>
    <p v-else>加载中…</p>
  </div>
</template>
"#;
    let needs_fmt = cfg.sections.iter().any(|(_, fs)| {
        fs.iter()
            .any(|(_, h)| matches!(h.as_deref(), Some("money") | Some("datetime")))
    });
    template
        .replace("%%ENTITY%%", &entity_name)
        .replace(
            "%%FMT_IMPORT%%",
            if needs_fmt {
                "import { fmtDateTime, fmtMoney } from \"../format\";\n"
            } else {
                ""
            },
        )
        .replace("%%SNAKE_E%%", &snake_e)
        .replace("%%TITLE%%", &escape_html(&title))
        .replace("%%ROWS%%", &rows)
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn simple_vue(page: &crate::dsl::PageDecl, body: &str) -> String {
    let title = pick_message(&page.title, &page.name);
    format!(
        "<template><div class=\"page\"><h1>{}</h1><p>{}</p></div></template>\n",
        escape_html(&title),
        body
    )
}

fn readme(app: &str) -> String {
    format!("# {app} Vue frontend (generated by Forma)\n\n```bash\nnpm install\nnpm run dev\n```\n")
}
