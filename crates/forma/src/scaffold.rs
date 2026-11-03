//! `forma init` 项目脚手架 —— 生成 v0.3 DSL 工程（一条命令进入全管线）。

use anyhow::{bail, Result};
use std::path::Path;

const FORMA_YAML: &str = r#"project:
  name: my-app
  version: 0.1.0

source: ["forma/**/*.fm"]

targets:
  db:
    engine: mysql
    out: db
  backend:
    language: rust
    framework: axum
    database: sqlx
    out: generated/backend
  web:
    framework: react
    router: tanstack
    data: tanstack-query
    ui: shadcn
    out: generated/web
  mobile:
    framework: flutter
    out: app_mobile
"#;

const APP_FM: &str = r#"// Forma DSL —— 起步工程：一个极简博客（作者/文章）
// 改完 .fm 后：forma validate && forma build
//   - db/          不可变基线 + 版本化迁移链 + 确定性种子数据（gen-seed）
//   - generated/   Rust 后端（axum）+ React Web（TanStack）
//   - app_mobile/  Flutter 客户端（Android / iOS 同源）
// 案例数据不是 AI 数据：全部由编译器从 DSL 确定性推导，可重放、可 hash 校验。

app myapp {
  version = "0.1.0"
  locale  = ["zh", "en"]
}

enum PostStatus {
  draft     @label { zh "草稿" en "Draft" }
  published @label { zh "已发布" en "Published" }
}

entity Author {
  id        Uuid       @id @default(uuid)
  name      String(50) @required
  email     Email      @required @unique
  createdAt Timestamp  @default(now) @readonly
}

entity Post {
  id        Uuid       @id @default(uuid)
  author    Author     @required
  title     String(120) @required
  status    PostStatus @default(draft)
  views     Int        @default(0)
  createdAt Timestamp  @default(now) @readonly

  index [author]
}

// 规则写一次：后端 Rust 权威校验 + Web zod + Flutter 表单校验三端同源
rules MyappRules {
  field Post.title {
    required { zh "标题不能为空" en "Title is required" }
  }
  guard Post.delete {
    deny status == "published"
      reason { zh "已发布文章不可删除，请先下线" en "Published posts cannot be deleted" }
  }
}

api Authors {
  entity Author
  operations [list get create update delete]
  list { search [name] sort [createdAt desc] }
}

api Posts {
  entity Post
  operations [list get create update delete]
  list {
    filters [author, status]
    search  [title]
    sort    [createdAt desc]
    paginate offset
  }
}

// ── 页面（目标无关原语 → shadcn/ui / Material） ──

page AuthorList {
  route "/authors"
  title { zh "作者" en "Authors" }
  table {
    query   Authors.list
    columns [name, email, createdAt: datetime]
    search  [name]
    actions {
      create → AuthorEdit
      row    [edit → AuthorEdit, delete → Authors.delete confirm]
    }
  }
}

page AuthorEdit {
  route "/authors/{id}"
  title { zh "编辑作者" en "Edit Author" }
  form Author {
    layout stack {
      name  Input @required
      email Input email
    }
    submit → Authors.update redirect "/authors"
    submit create → Authors.create redirect "/authors"
  }
}

page PostList {
  route "/posts"
  title { zh "文章" en "Posts" }
  table {
    query   Posts.list
    columns [title, author: ref, status: badge, views, createdAt: datetime]
    filter  author as select
    filter  status as select
    search  [title]
    actions {
      create → PostEdit
      row    [edit → PostEdit, delete → Posts.delete confirm]
    }
  }
}

page PostEdit {
  route "/posts/{id}"
  title { zh "编辑文章" en "Edit Post" }
  form Post {
    layout stack {
      author Select from Author @required
      title  Input @required
      status Select from PostStatus
      views  Input
    }
    validation [MyappRules]
    submit → Posts.update redirect "/posts"
    submit create → Posts.create redirect "/posts"
  }
}
"#;

const README: &str = r#"# my-app

由 [Forma](https://github.com/huifer/forma) 生成的 DSL 工程：一套 `.fm` 描述
数据/规则/API/页面，确定性编译为 MySQL 迁移链、Rust 后端、React Web、Flutter 移动端、
OpenAPI、图与案例数据。

## 快速开始

```bash
forma validate           # DSL 语义检查
forma build              # 全目标产物
forma migrate status     # 迁移链状态
forma migrate verify     # 迁移链完整性 + 重放校验
```

产物布局：

```
forma/          DSL 源（唯一事实源——改这里）
db/             0001_init.sql + 版本化 upgrade/rollback + MIGRATIONS.md + seed/（案例数据）
generated/      Rust 后端 + React Web + OpenAPI + 图（gitignored）
app_mobile/     Flutter 客户端（Android / iOS 同源，入库）
```

真实 MySQL 起后端：

```bash
mysql -uroot -p -e "CREATE DATABASE myapp"
mysql -uroot -p myapp < db/migrations/0001_init.sql
mysql -uroot -p myapp < db/seed/seed_0.1.0.sql
cd generated/backend
DATABASE_URL="mysql://root:密码@127.0.0.1:3306/myapp" cargo run
```

Web：`cd generated/web && npm i && npm run dev`（dev server 代理 `/api`）。
移动端：`cd app_mobile && flutter pub get && flutter run`。
"#;

const GITIGNORE: &str = r#"# Forma
generated/
.forma/

# 依赖与构建
node_modules/
target/
build/
.dart_tool/
"#;

/// 生成 v0.3 DSL 工程脚手架。
pub fn scaffold_project(dir: &Path) -> Result<()> {
    let yaml = dir.join(crate::manifest::MANIFEST_FILE);
    if yaml.exists() {
        bail!("forma.yaml already exists at {}", yaml.display());
    }
    crate::util::write_text(&yaml, FORMA_YAML)?;
    crate::util::write_text(&dir.join("forma/app.fm"), APP_FM)?;
    crate::util::write_text(&dir.join("README.md"), README)?;
    crate::util::write_text(&dir.join(".gitignore"), GITIGNORE)?;
    Ok(())
}
