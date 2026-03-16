CREATE TABLE session
(
    id         TEXT PRIMARY KEY NOT NULL,
    created_at TIMESTAMP        NOT NULL DEFAULT (datetime('now')),
    updated_at TIMESTAMP        NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE session_provider_config
(
    session_id  TEXT      NOT NULL,
    provider_id TEXT      NOT NULL,
    config_json TEXT      NOT NULL,
    created_at  TIMESTAMP NOT NULL DEFAULT (datetime('now')),
    updated_at  TIMESTAMP NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (session_id, provider_id),
    FOREIGN KEY (session_id) REFERENCES session (id) ON DELETE CASCADE
);
