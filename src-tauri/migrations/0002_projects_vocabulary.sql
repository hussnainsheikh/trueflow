CREATE TABLE projects (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    description TEXT NOT NULL DEFAULT '',
    default_voice_id TEXT,
    default_mode_id TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    archived_at TEXT
);

CREATE TABLE active_project (
    singleton INTEGER PRIMARY KEY NOT NULL CHECK (singleton = 1),
    project_id TEXT NOT NULL UNIQUE,
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE RESTRICT
);

CREATE TABLE vocabulary_entries (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL,
    term TEXT NOT NULL CHECK (length(trim(term)) > 0),
    pronunciation_hint TEXT,
    kind TEXT NOT NULL CHECK (
        kind IN ('person', 'place', 'organization', 'object', 'technical', 'other')
    ),
    description TEXT,
    case_sensitive INTEGER NOT NULL DEFAULT 0 CHECK (case_sensitive IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX vocabulary_entries_project_term_nocase
ON vocabulary_entries(project_id, term COLLATE NOCASE);

CREATE TABLE vocabulary_aliases (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    vocabulary_entry_id TEXT NOT NULL,
    value TEXT NOT NULL CHECK (length(trim(value)) > 0),
    FOREIGN KEY (vocabulary_entry_id) REFERENCES vocabulary_entries(id) ON DELETE CASCADE,
    UNIQUE (vocabulary_entry_id, value COLLATE NOCASE)
);

CREATE TABLE vocabulary_forbidden_variants (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    vocabulary_entry_id TEXT NOT NULL,
    value TEXT NOT NULL CHECK (length(trim(value)) > 0),
    FOREIGN KEY (vocabulary_entry_id) REFERENCES vocabulary_entries(id) ON DELETE CASCADE,
    UNIQUE (vocabulary_entry_id, value COLLATE NOCASE)
);

CREATE INDEX vocabulary_entries_project_id
ON vocabulary_entries(project_id);

INSERT INTO projects (id, name, description)
SELECT '00000000-0000-4000-8000-000000000001', 'General',
       'Default project for everyday dictation.'
WHERE NOT EXISTS (SELECT 1 FROM projects);

INSERT INTO active_project (singleton, project_id)
SELECT 1, id
FROM projects
WHERE archived_at IS NULL
ORDER BY created_at, id
LIMIT 1;
