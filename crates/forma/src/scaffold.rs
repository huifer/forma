//! `forma init` 项目脚手架。

use anyhow::{bail, Result};
use std::path::Path;

const FORMA_YAML: &str = r#"project:
  name: my-app
  version: 0.1.0

specifications:
  database:
    - path: specs/database/schema.sql
      dialect: sqlite

  schemas:
    - path: specs/schemas/user.schema.json
      format: json-schema
      dialect: "2020-12"

  api:
    - path: specs/api/openapi.yaml
      version: "3.1"

  ui:
    - path: specs/ui/user-form.ui.json
      format: json-forms

targets:
  typescript: generated/types
  backend:
    framework: hono
    out: generated/backend
  database:
    dialect: sqlite
    out: generated/database
  frontend:
    framework: react
    out: generated/frontend
"#;

const SCHEMA_SQL: &str = r#"-- Database structure is owned by SQL DDL — no Forma-specific syntax.
CREATE TABLE users (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    email TEXT NOT NULL UNIQUE,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TEXT NOT NULL
);

CREATE INDEX idx_users_status ON users(status);
"#;

const USER_SCHEMA: &str = r#"{
  "$id": "User",
  "type": "object",
  "required": ["name", "email", "status", "createdAt"],
  "properties": {
    "id": { "type": "string", "description": "Unique identifier" },
    "name": { "type": "string", "minLength": 1, "maxLength": 100 },
    "email": { "type": "string", "format": "email" },
    "status": { "type": "string", "enum": ["active", "suspended", "closed"] },
    "createdAt": { "type": "string", "format": "date-time" }
  },
  "x-forma-entity": "forma://project/my-app/entity/User"
}
"#;

const OPENAPI: &str = r#"openapi: "3.1.0"
info:
  title: my-app API
  version: 0.1.0
paths:
  /users:
    post:
      operationId: createUser
      summary: Create a user
      requestBody:
        required: true
        content:
          application/json:
            schema:
              $ref: '#/components/schemas/User'
      responses:
        '201':
          description: Created
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/User'
  /users/{id}:
    get:
      operationId: getUserById
      summary: Get a user by id
      parameters:
        - name: id
          in: path
          required: true
          schema:
            type: string
      responses:
        '200':
          description: OK
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/User'
components:
  schemas:
    User:
      type: object
      x-forma-entity: forma://project/my-app/entity/User
      required: [name, email, status, createdAt]
      properties:
        id:
          type: string
        name:
          type: string
        email:
          type: string
        status:
          type: string
          enum: [active, suspended, closed]
        createdAt:
          type: string
          format: date-time
"#;

const UI_SCHEMA: &str = r##"{
  "$id": "user-form",
  "schema": "User",
  "type": "VerticalLayout",
  "elements": [
    { "type": "Control", "scope": "#/properties/name" },
    { "type": "Control", "scope": "#/properties/email" },
    { "type": "Control", "scope": "#/properties/status" }
  ]
}
"##;

pub fn scaffold_project(project_dir: &Path) -> Result<()> {
    if project_dir.join("forma.yaml").is_file() {
        bail!("forma.yaml already exists in {}", project_dir.display());
    }
    let files: Vec<(&str, &str)> = vec![
        ("forma.yaml", FORMA_YAML),
        ("specs/database/schema.sql", SCHEMA_SQL),
        ("specs/schemas/user.schema.json", USER_SCHEMA),
        ("specs/api/openapi.yaml", OPENAPI),
        ("specs/ui/user-form.ui.json", UI_SCHEMA),
        (".gitignore", "node_modules/\ngenerated/\n.forma/\n"),
    ];
    for (rel, content) in files {
        crate::util::write_text(&project_dir.join(rel), content)?;
    }
    Ok(())
}
