# Migrations — 数据库版本台账

> 由 Forma `gen-migrate` 自动生成，请勿手工编辑；DSL 变更后 `forma build` 会追加新版本。
> 应用/回滚：`forma migrate up|down`（详见 `forma migrate --help`）。

| # | 目标版本 | 升级 | 回滚 | 状态 | 变更摘要 |
|---|----------|------|------|------|----------|
| 1 | 0.3.0 | 0001_init.sql | — | ⏳ pending | 迁移 #1 已入库 |
| 2 | 0.2.0 | 0002_upgrade.sql | 0002_rollback.sql | ⏳ pending | 迁移 #2 已入库 |
| 3 | 0.3.0 | 0003_upgrade.sql | 0003_rollback.sql | ⏳ pending | 迁移 #3 已入库 |
| 4 | 0.3.0 | 0004_upgrade.sql | 0004_rollback.sql | ⏳ pending | 迁移 #4 已入库 |
| 5 | 0.3.0 | 0005_upgrade.sql | 0005_rollback.sql | ⏳ pending | 迁移 #5 已入库 |

---

## #1 — upgrade to v0.3.0

变更：
- 迁移 #1 已入库

```sql
-- UP
CREATE TABLE schools (
  id CHAR(36) NOT NULL,
  name VARCHAR(100) NOT NULL,
  city VARCHAR(50) NULL,
  created_at DATETIME(3) NOT NULL,
  PRIMARY KEY (id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE students (
  id CHAR(36) NOT NULL,
  school_id CHAR(36) NOT NULL,
  name VARCHAR(50) NOT NULL,
  email VARCHAR(320) NOT NULL,
  age BIGINT NULL,
  created_at DATETIME(3) NOT NULL,
  PRIMARY KEY (id),
  UNIQUE KEY uq_students_email (email),
  KEY idx_students_school_id_name (school_id, name),
  CONSTRAINT fk_students_school_id FOREIGN KEY (school_id) REFERENCES schools (id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE courses (
  id CHAR(36) NOT NULL,
  school_id CHAR(36) NOT NULL,
  title VARCHAR(120) NOT NULL,
  capacity BIGINT NOT NULL,
  created_at DATETIME(3) NOT NULL,
  PRIMARY KEY (id),
  KEY idx_courses_school_id (school_id),
  CONSTRAINT fk_courses_school_id FOREIGN KEY (school_id) REFERENCES schools (id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE enrollments (
  id CHAR(36) NOT NULL,
  student_id CHAR(36) NOT NULL,
  course_id CHAR(36) NOT NULL,
  status VARCHAR(64) NOT NULL,
  enrolled_at DATETIME(3) NOT NULL,
  created_at DATETIME(3) NOT NULL,
  PRIMARY KEY (id),
  UNIQUE KEY uq_enrollments_student_id_course_id (student_id, course_id),
  KEY idx_enrollments_student_id_course_id (student_id, course_id),
  CONSTRAINT fk_enrollments_student_id FOREIGN KEY (student_id) REFERENCES students (id),
  CONSTRAINT fk_enrollments_course_id FOREIGN KEY (course_id) REFERENCES courses (id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;
```

## #2 — upgrade to v0.2.0

变更：
- 迁移 #2 已入库

```sql
-- UP
CREATE TABLE elections (
  id CHAR(36) NOT NULL,
  title VARCHAR(200) NOT NULL,
  status VARCHAR(64) NOT NULL,
  starts_at DATETIME(3) NOT NULL,
  ends_at DATETIME(3) NOT NULL,
  PRIMARY KEY (id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;
CREATE TABLE candidates (
  id CHAR(36) NOT NULL,
  election_id CHAR(36) NOT NULL,
  student_id CHAR(36) NOT NULL,
  slogan VARCHAR(500) NOT NULL,
  PRIMARY KEY (id),
  KEY idx_candidates_election_id (election_id),
  CONSTRAINT fk_candidates_election_id FOREIGN KEY (election_id) REFERENCES elections (id),
  CONSTRAINT fk_candidates_student_id FOREIGN KEY (student_id) REFERENCES students (id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;
CREATE TABLE vote_records (
  id CHAR(36) NOT NULL,
  election_id CHAR(36) NOT NULL,
  candidate_id CHAR(36) NOT NULL,
  voter_id CHAR(36) NOT NULL,
  PRIMARY KEY (id),
  UNIQUE KEY uq_vote_records_election_id_voter_id (election_id, voter_id),
  CONSTRAINT fk_vote_records_election_id FOREIGN KEY (election_id) REFERENCES elections (id),
  CONSTRAINT fk_vote_records_candidate_id FOREIGN KEY (candidate_id) REFERENCES candidates (id),
  CONSTRAINT fk_vote_records_voter_id FOREIGN KEY (voter_id) REFERENCES students (id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;
ALTER TABLE enrollments ADD COLUMN note TEXT NULL;
ALTER TABLE enrollments ADD INDEX idx_enrollments_status (status);
```
```sql
-- DOWN（回滚）
ALTER TABLE enrollments DROP INDEX idx_enrollments_status;
ALTER TABLE enrollments DROP COLUMN note;
DROP TABLE IF EXISTS vote_records;
DROP TABLE IF EXISTS candidates;
DROP TABLE IF EXISTS elections;
```

## #3 — upgrade to v0.3.0

变更：
- 迁移 #3 已入库

```sql
-- UP
ALTER TABLE students ADD COLUMN phone VARCHAR(20) NULL;
ALTER TABLE courses MODIFY COLUMN title VARCHAR(200) NOT NULL;
ALTER TABLE elections ADD COLUMN remark TEXT NULL;
ALTER TABLE vote_records ADD INDEX idx_vote_records_voter_id (voter_id);
```
```sql
-- DOWN（回滚）
ALTER TABLE vote_records DROP INDEX idx_vote_records_voter_id;
ALTER TABLE elections DROP COLUMN remark;
ALTER TABLE courses MODIFY COLUMN title VARCHAR(120) NOT NULL;
ALTER TABLE students DROP COLUMN phone;
```

## #4 — upgrade to v0.3.0

变更：
- 迁移 #4 已入库

```sql
-- UP
CREATE TABLE users (
  id CHAR(36) NOT NULL,
  username VARCHAR(64) NOT NULL,
  password_hash VARCHAR(255) NOT NULL,
  role VARCHAR(64) NOT NULL,
  created_at DATETIME(3) NOT NULL,
  PRIMARY KEY (id),
  UNIQUE KEY uq_users_username (username)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;
CREATE TABLE audit_logs (
  id CHAR(36) NOT NULL,
  action VARCHAR(191) NOT NULL,
  entity VARCHAR(191) NOT NULL,
  entity_id CHAR(36) NOT NULL,
  actor VARCHAR(191) NULL,
  payload JSON NULL,
  created_at DATETIME(3) NOT NULL,
  PRIMARY KEY (id),
  KEY idx_audit_logs_created_at (created_at),
  KEY idx_audit_logs_entity_entity_id (entity, entity_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;
```
```sql
-- DOWN（回滚）
DROP TABLE IF EXISTS audit_logs;
DROP TABLE IF EXISTS users;
```

## #5 — upgrade to v0.3.0

变更：
- 迁移 #5 已入库

```sql
-- UP
ALTER TABLE audit_logs ADD COLUMN idempotency_key CHAR(36) NULL;
```
```sql
-- DOWN（回滚）
ALTER TABLE audit_logs DROP COLUMN idempotency_key;
```

