CREATE TABLE workspaces (
    id TEXT PRIMARY KEY,
    revision BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE reviews (
    workspace_id TEXT NOT NULL REFERENCES workspaces(id),
    id TEXT NOT NULL,
    position BIGINT NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('local','github')),
    repository TEXT NOT NULL,
    pr INTEGER NOT NULL,
    head TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('draft','COMMENT','APPROVE','REQUEST_CHANGES')),
    body TEXT NOT NULL,
    actor_name TEXT NOT NULL,
    actor_kind TEXT NOT NULL CHECK(actor_kind IN ('human','agent')),
    remote_id BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY(workspace_id,id),
    UNIQUE(workspace_id,position)
);
CREATE TABLE comments (
    workspace_id TEXT NOT NULL REFERENCES workspaces(id),
    id TEXT NOT NULL,
    position BIGINT NOT NULL,
    thread_id TEXT,
    review_id TEXT,
    resolved BOOLEAN NOT NULL DEFAULT FALSE,
    path TEXT NOT NULL,
    line INTEGER NOT NULL CHECK(line > 0),
    side TEXT NOT NULL CHECK(side IN ('LEFT','RIGHT')),
    body TEXT NOT NULL,
    actor_name TEXT NOT NULL,
    actor_kind TEXT NOT NULL CHECK(actor_kind IN ('human','agent')),
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY(workspace_id,id),
    UNIQUE(workspace_id,position),
    FOREIGN KEY(workspace_id,review_id) REFERENCES reviews(workspace_id,id),
    FOREIGN KEY(workspace_id,thread_id) REFERENCES comments(workspace_id,id)
);
CREATE INDEX comments_by_review ON comments(workspace_id,review_id,position);
CREATE INDEX comments_by_thread ON comments(workspace_id,thread_id,position);
CREATE INDEX comments_by_path ON comments(workspace_id,path,line);
