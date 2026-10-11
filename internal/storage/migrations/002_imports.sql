CREATE TABLE legacy_imports (
    workspace_id TEXT NOT NULL REFERENCES workspaces(id),
    source TEXT NOT NULL,
    checksum TEXT NOT NULL,
    imported_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY(workspace_id,source)
);
