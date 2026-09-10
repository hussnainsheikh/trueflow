CREATE TABLE product_settings (
    singleton INTEGER PRIMARY KEY NOT NULL CHECK (singleton = 1),
    onboarding_state TEXT NOT NULL DEFAULT 'first_run'
        CHECK (onboarding_state IN ('first_run', 'in_progress', 'ready')),
    onboarding_step TEXT NOT NULL DEFAULT 'welcome'
        CHECK (onboarding_step IN ('welcome', 'microphone', 'accessibility', 'account', 'shortcut', 'first_dictation', 'behavior', 'complete')),
    history_enabled INTEGER NOT NULL DEFAULT 1 CHECK (history_enabled IN (0, 1)),
    analytics_enabled INTEGER NOT NULL DEFAULT 0 CHECK (analytics_enabled IN (0, 1)),
    show_overlay INTEGER NOT NULL DEFAULT 1 CHECK (show_overlay IN (0, 1)),
    dictation_shortcut TEXT NOT NULL DEFAULT 'Option+Space',
    rewrite_shortcut TEXT NOT NULL DEFAULT 'Option+Shift+Space',
    dictation_paused INTEGER NOT NULL DEFAULT 0 CHECK (dictation_paused IN (0, 1)),
    installation_id TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

INSERT INTO product_settings (singleton, installation_id)
VALUES (1, lower(hex(randomblob(16))));

CREATE TABLE dictation_history (
    id TEXT PRIMARY KEY NOT NULL,
    operation_id TEXT NOT NULL UNIQUE,
    kind TEXT NOT NULL CHECK (kind IN ('dictation', 'selection_rewrite')),
    project_id TEXT,
    project_name TEXT,
    voice_id TEXT,
    voice_name TEXT,
    mode_id TEXT,
    mode_name TEXT,
    source_app TEXT,
    final_text TEXT NOT NULL CHECK (length(final_text) > 0),
    status TEXT NOT NULL CHECK (status IN ('completed', 'recovered', 'copied', 'failed')),
    error_code TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE SET NULL,
    FOREIGN KEY (voice_id) REFERENCES voices(id) ON DELETE SET NULL,
    FOREIGN KEY (mode_id) REFERENCES modes(id) ON DELETE SET NULL
);

CREATE INDEX dictation_history_created_at
ON dictation_history(created_at DESC, id DESC);

CREATE TABLE account_metadata (
    singleton INTEGER PRIMARY KEY NOT NULL CHECK (singleton = 1),
    provider TEXT NOT NULL,
    user_id TEXT NOT NULL,
    email TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
