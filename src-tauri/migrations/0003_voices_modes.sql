CREATE TABLE voices (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    description TEXT NOT NULL DEFAULT '',
    instructions TEXT NOT NULL CHECK (length(trim(instructions)) > 0),
    avoidances TEXT NOT NULL DEFAULT '',
    is_default INTEGER NOT NULL DEFAULT 0 CHECK (is_default IN (0, 1)),
    is_builtin INTEGER NOT NULL DEFAULT 0 CHECK (is_builtin IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX voices_scope_name_nocase
ON voices(COALESCE(project_id, ''), name COLLATE NOCASE);

CREATE TABLE voice_examples (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    voice_id TEXT NOT NULL,
    position INTEGER NOT NULL CHECK (position >= 0),
    example TEXT NOT NULL CHECK (length(trim(example)) > 0),
    FOREIGN KEY (voice_id) REFERENCES voices(id) ON DELETE CASCADE,
    UNIQUE (voice_id, position)
);

CREATE TABLE modes (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL COLLATE NOCASE UNIQUE,
    description TEXT NOT NULL,
    system_instructions TEXT NOT NULL CHECK (length(trim(system_instructions)) > 0),
    requires_voice INTEGER NOT NULL DEFAULT 0 CHECK (requires_voice IN (0, 1)),
    supports_selection INTEGER NOT NULL DEFAULT 0 CHECK (supports_selection IN (0, 1)),
    is_builtin INTEGER NOT NULL DEFAULT 1 CHECK (is_builtin IN (0, 1)),
    position INTEGER NOT NULL UNIQUE,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

INSERT INTO voices (
    id, project_id, name, description, instructions, avoidances, is_default, is_builtin
) VALUES
(
    '10000000-0000-4000-8000-000000000001', NULL, 'My Voice',
    'A neutral personal writing Voice that preserves the speaker''s wording.',
    'Preserve the writer''s natural vocabulary, intent, point of view, and sentence rhythm. Make only changes required by the selected Mode.',
    'Do not add generic corporate language, inflated claims, or facts the writer did not provide.',
    1, 1
),
(
    '10000000-0000-4000-8000-000000000002', NULL, 'Narrator',
    'A clear fiction narrator suitable for readable prose drafts.',
    'Write as a confident fiction narrator. Use concrete language, varied sentence rhythm, consistent point of view, and restrained sensory detail.',
    'Avoid generic AI phrasing, excessive adjectives, exposition not present in the source, and unnecessary moral commentary.',
    0, 1
);

INSERT INTO modes (
    id, name, description, system_instructions, requires_voice, supports_selection, is_builtin, position
) VALUES
(
    '20000000-0000-4000-8000-000000000001', 'Clean Dictation',
    'Light cleanup that closely preserves the spoken wording.',
    'Preserve meaning and wording closely. Correct punctuation, capitalization, obvious grammar, fillers, and false starts only. Do not creatively rewrite.',
    0, 0, 1, 1
),
(
    '20000000-0000-4000-8000-000000000002', 'Draft',
    'Turn spoken thoughts into readable prose in the selected Voice.',
    'Turn the spoken thought into coherent readable prose. Respect the selected Voice and project facts. Improve structure without inventing plot facts.',
    1, 0, 1, 2
),
(
    '20000000-0000-4000-8000-000000000003', 'Dialogue',
    'Produce dialogue appropriate to the selected character Voice.',
    'Render the content as natural character dialogue in the selected Voice. Preserve intent and known facts. Do not invent unrelated action or plot.',
    1, 0, 1, 3
),
(
    '20000000-0000-4000-8000-000000000004', 'Brainstorm',
    'Organize spoken ideas while preserving their exploratory nature.',
    'Preserve every useful idea, group related thoughts clearly, and make uncertainty explicit. Do not turn possibilities into established facts.',
    0, 0, 1, 4
),
(
    '20000000-0000-4000-8000-000000000005', 'Rewrite Selection',
    'Reserved for the Milestone 6 selected-text rewrite workflow.',
    'Transform only explicitly selected source text according to the spoken instruction.',
    1, 1, 1, 5
);

UPDATE projects
SET default_voice_id = COALESCE(default_voice_id, '10000000-0000-4000-8000-000000000001'),
    default_mode_id = COALESCE(default_mode_id, '20000000-0000-4000-8000-000000000001'),
    updated_at = CURRENT_TIMESTAMP;
