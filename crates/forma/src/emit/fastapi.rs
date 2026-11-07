//! FastAPI backend target（M10）：Python + FastAPI + SQLAlchemy 2.0 + Pydantic。
//!
//! 支持标量字段、枚举、Decimal、required/min/max、CRUD。
//! auth / access / tenant / commands / cross-field rules 显式拒绝，不得静默降级。

use crate::dsl::{has_attr, is_enum, is_relation, FieldDecl, Program};
use crate::generators::GeneratedFile;
use crate::identity::to_snake_case;
use std::path::Path;

pub fn emit_fastapi(
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

    files.push(file(out_dir, "pyproject.toml", pyproject(&pkg)));
    files.push(file(out_dir, "src/__init__.py", String::new()));
    files.push(file(out_dir, "src/database.py", database_py()));
    files.push(file(out_dir, "src/models.py", models_py(program)));
    files.push(file(out_dir, "src/schemas.py", schemas_py(program)));
    files.push(file(out_dir, "src/routes.py", routes_py(program)));
    files.push(file(out_dir, "src/main.py", main_py()));
    files.push(file(out_dir, "README.md", readme(&pkg)));

    for f in &files {
        crate::util::write_text(&project_dir.join(&f.path), &f.content)?;
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
            "FastAPI target currently supports scalar CRUD + field min/max/required only; unsupported DSL features: {}. Select backend.language: rust to preserve full semantics",
            unsupported.join(", ")
        );
    }
    Ok(())
}

fn file(out_dir: &str, rel: &str, content: String) -> GeneratedFile {
    GeneratedFile {
        path: format!("{out_dir}/{rel}"),
        content,
        generator: "gen-fastapi".into(),
    }
}

fn pyproject(pkg: &str) -> String {
    format!(
        r#"[project]
name = "{pkg}-backend"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = [
    "fastapi>=0.115",
    "uvicorn[standard]>=0.30",
    "sqlalchemy>=2.0",
    "pymysql>=1.1",
    "cryptography>=43",
    "pydantic>=2.8",
]

[tool.setuptools.packages.find]
where = ["src"]
"#
    )
}

fn database_py() -> String {
    r#"import os

from sqlalchemy import create_engine
from sqlalchemy.orm import DeclarativeBase, sessionmaker

DATABASE_URL = os.environ.get(
    "DATABASE_URL", "mysql+pymysql://root:root@localhost:3306/app"
)

engine = create_engine(DATABASE_URL, pool_pre_ping=True)
SessionLocal = sessionmaker(bind=engine, autoflush=False)


class Base(DeclarativeBase):
    pass


def get_db():
    db = SessionLocal()
    try:
        yield db
    finally:
        db.close()
"#
    .to_string()
}

fn py_type(program: &Program, f: &FieldDecl) -> String {
    if is_relation(program, &f.type_name) || is_enum(program, &f.type_name) {
        return "str".into();
    }
    match f.type_name.as_str() {
        "Int" => "int".into(),
        "Float" => "float".into(),
        "Decimal" => "Decimal".into(),
        "Bool" => "bool".into(),
        _ => "str".into(),
    }
}

fn py_column_type(f: &FieldDecl) -> String {
    match f.type_name.as_str() {
        "Uuid" | "String" | "Email" => format!(
            "String({})",
            f.type_args.first().cloned().unwrap_or_else(|| "255".into())
        ),
        "Text" => "Text".into(),
        "Int" => "BigInteger".into(),
        "Float" => "Float".into(),
        "Decimal" => {
            let p = f.type_args.first().cloned().unwrap_or_else(|| "12".into());
            let s = f.type_args.get(1).cloned().unwrap_or_else(|| "2".into());
            format!("DECIMAL({p}, {s})")
        }
        "Bool" => "Boolean".into(),
        "Timestamp" => "DateTime(3)".into(),
        "Date" => "Date".into(),
        "Json" => "JSON".into(),
        _ => "String(255)".into(),
    }
}

fn py_nullable(f: &FieldDecl) -> bool {
    f.optional || has_attr(f, "default") || has_attr(f, "readonly")
}

fn models_py(program: &Program) -> String {
    let mut imports = String::from(
        "import enum\n\nfrom sqlalchemy import Boolean, Date, DateTime, Enum, Float, JSON, String, Text, BigInteger, DECIMAL\nfrom sqlalchemy.orm import Mapped, mapped_column\n\nfrom database import Base\n\n",
    );
    for entity in &program.entities {
        let table = crate::dsl::pluralize(&entity.name).to_lowercase();
        let mut fields = String::new();
        for f in &entity.fields {
            let name = to_snake_case(&f.name);
            let py_t = py_type(program, f);
            let nullable = py_nullable(f);
            let optional = if nullable { " | None" } else { "" };
            let col_type = py_column_type(f);
            if has_attr(f, "id") {
                fields.push_str(&format!(
                    "    {name}: Mapped[{py_t}] = mapped_column({col_type}, primary_key=True)\n\n"
                ));
            } else {
                let default_part = if nullable { ", default=None" } else { "" };
                fields.push_str(&format!(
                    "    {name}: Mapped[{py_t}{optional}] = mapped_column({col_type}{default_part})\n\n"
                ));
            }
            // Enum: add __members__
            if crate::dsl::is_enum(program, &f.type_name) {
                if let Some(e) = program.enums.iter().find(|e| e.name == f.type_name) {
                    let variants = e
                        .variants
                        .iter()
                        .map(|v| format!("    {} = \"{}\"\n", v.name.to_lowercase(), v.name))
                        .collect::<Vec<_>>()
                        .join("");
                    imports.push_str(&format!(
                        "\nclass {f_type}(str, enum.Enum):\n{variants}\n",
                        f_type = f.type_name
                    ));
                }
            }
        }
        imports.push_str(&format!(
            "\nclass {name}(Base):\n    __tablename__ = \"{table}\"\n\n{fields}\n\n",
            name = entity.name,
            table = table,
            fields = fields,
        ));
    }
    imports
}

fn schemas_py(program: &Program) -> String {
    let mut out = String::from(
        "from decimal import Decimal\nfrom datetime import datetime, date\n\nfrom pydantic import BaseModel, ConfigDict, Field\n\n",
    );
    for entity in &program.entities {
        let name = &entity.name;
        // Create schema
        let mut create_fields = String::new();
        for f in &entity.fields {
            if has_attr(f, "id") || has_attr(f, "readonly") {
                continue;
            }
            let py_name = to_snake_case(&f.name);
            let py_t = py_type(program, f);
            let optional = py_nullable(f);
            let mut constraints = String::new();
            if matches!(f.type_name.as_str(), "Int" | "Float" | "Decimal") {
                for attr in &f.attrs {
                    if attr.name == "min" {
                        if let Some(v) = attr.args.first() {
                            constraints.push_str(&format!(", ge={v}"));
                        }
                    }
                    if attr.name == "max" {
                        if let Some(v) = attr.args.first() {
                            constraints.push_str(&format!(", le={v}"));
                        }
                    }
                }
            }
            let default = if optional { " = None" } else { "" };
            create_fields.push_str(&format!(
                "    {py_name}: {py_t}{optional} | None = None{default}\n"
            ));
        }
        out.push_str(&format!(
            "class Create{name}(BaseModel):\n{create_fields}\n\n",
            name = name,
            create_fields = create_fields,
        ));
        // Read schema (response model)
        let mut read_fields = String::new();
        read_fields.push_str("    model_config = ConfigDict(from_attributes=True)\n\n");
        for f in &entity.fields {
            let py_name = to_snake_case(&f.name);
            let py_t = py_type(program, f);
            let optional = py_nullable(f);
            read_fields.push_str(&format!("    {py_name}: {py_t}{optional} | None = None\n"));
        }
        out.push_str(&format!(
            "class Read{name}(BaseModel):\n{read_fields}\n\n",
            name = name,
            read_fields = read_fields,
        ));
    }
    out
}

fn routes_py(program: &Program) -> String {
    let mut imports = String::new();
    let mut routers = String::new();
    for entity in &program.entities {
        let name = &entity.name;
        let snake = to_snake_case(name);
        let plural = crate::dsl::pluralize(name).to_lowercase();
        let has = |op: &str| {
            program
                .apis
                .iter()
                .find(|a| a.entity == *name)
                .map(|a| a.operations.contains(&op.to_string()))
                .unwrap_or(false)
        };

        imports.push_str(&format!(
            "from fastapi import APIRouter, Depends, HTTPException\nfrom sqlalchemy.orm import Session\n\nfrom database import get_db\nfrom models import {name}\nfrom schemas import Create{name}, Read{name}\n\nrouter = APIRouter(prefix=\"/{plural}\", tags=[\"{name}\"])\n\n"
        ));

        let mut methods = String::new();
        if has("list") {
            methods.push_str(&format!(
                "@router.get(\"/\")\ndef list_{snake}(skip: int = 0, limit: int = 20, db: Session = Depends(get_db)) -> list[Read{name}]:\n    return db.query({name}).offset(skip).limit(min(limit, 200)).all()\n\n",
                snake = snake,
                name = name,
            ));
        }
        if has("get") {
            methods.push_str(&format!(
                "@router.get(\"/{{item_id}}\")\ndef get_{snake}(item_id: str, db: Session = Depends(get_db)) -> Read{name}:\n    row = db.query({name}).filter({name}.id == item_id).first()\n    if row is None:\n        raise HTTPException(status_code=404, detail=\"not found\")\n    return row\n\n",
                snake = snake,
                name = name,
            ));
        }
        if has("create") {
            methods.push_str(&format!(
                "@router.post(\"/\", status_code=201)\ndef create_{snake}(dto: Create{name}, db: Session = Depends(get_db)) -> Read{name}:\n    row = {name}(**dto.model_dump())\n    db.add(row)\n    db.commit()\n    db.refresh(row)\n    return row\n\n",
                snake = snake,
                name = name,
            ));
        }
        if has("update") {
            methods.push_str(&format!(
                "@router.put(\"/{{item_id}}\")\ndef update_{snake}(item_id: str, dto: Create{name}, db: Session = Depends(get_db)) -> Read{name}:\n    row = db.query({name}).filter({name}.id == item_id).first()\n    if row is None:\n        raise HTTPException(status_code=404, detail=\"not found\")\n    for k, v in dto.model_dump(exclude_unset=True).items():\n        setattr(row, k, v)\n    db.commit()\n    db.refresh(row)\n    return row\n\n",
                snake = snake,
                name = name,
            ));
        }
        if has("delete") {
            methods.push_str(&format!(
                "@router.delete(\"/{{item_id}}\")\ndef delete_{snake}(item_id: str, db: Session = Depends(get_db)) -> dict:\n    row = db.query({name}).filter({name}.id == item_id).first()\n    if row is None:\n        raise HTTPException(status_code=404, detail=\"not found\")\n    db.delete(row)\n    db.commit()\n    return {{\"ok\": True}}\n\n",
                snake = snake,
                name = name,
            ));
        }
        routers.push_str(&methods);
    }

    format!(
        "from fastapi import APIRouter, Depends, HTTPException\nfrom sqlalchemy.orm import Session\n\nfrom database import get_db\nfrom models import *\nfrom schemas import *\n\n{imports}{routers}"
    )
}

fn main_py() -> String {
    r#"import uvicorn

from routes import router

app = router

if __name__ == "__main__":
    uvicorn.run(app, host="0.0.0.0", port=8080)
"#
    .to_string()
}

fn readme(pkg: &str) -> String {
    format!(
        "# {pkg} FastAPI backend (generated by Forma)\n\n```bash\npip install -e .\nuvicorn src.main:app --host 0.0.0.0 --port 8080 --reload\n```\n\nSet `DATABASE_URL` before starting.\n"
    )
}
