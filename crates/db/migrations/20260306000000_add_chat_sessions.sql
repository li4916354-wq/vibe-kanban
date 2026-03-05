-- Add chat_sessions table for project-level chat (not tied to tasks/workspaces)
-- Chat sessions work on the current branch without creating worktrees

CREATE TABLE chat_sessions (
    id              BLOB PRIMARY KEY,
    project_id      BLOB NOT NULL,
    title           TEXT,
    pinned          INTEGER NOT NULL DEFAULT 0,
    executor        TEXT,
    created_at      TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
);

CREATE INDEX idx_chat_sessions_project_id ON chat_sessions(project_id);
CREATE INDEX idx_chat_sessions_project_pinned ON chat_sessions(project_id, pinned DESC, updated_at DESC);

-- Create chat_execution_processes to track execution for chat sessions
CREATE TABLE chat_execution_processes (
    id                  BLOB PRIMARY KEY,
    chat_session_id     BLOB NOT NULL,
    run_reason          TEXT NOT NULL DEFAULT 'codingagent'
                           CHECK (run_reason IN ('codingagent')),
    executor_action     TEXT NOT NULL DEFAULT '{}',
    status              TEXT NOT NULL DEFAULT 'running'
                           CHECK (status IN ('running','completed','failed','killed')),
    exit_code           INTEGER,
    started_at          TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    completed_at        TEXT,
    created_at          TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    updated_at          TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    FOREIGN KEY (chat_session_id) REFERENCES chat_sessions(id) ON DELETE CASCADE
);

CREATE INDEX idx_chat_execution_processes_session_id ON chat_execution_processes(chat_session_id);
CREATE INDEX idx_chat_execution_processes_status ON chat_execution_processes(status);

-- Create chat_coding_agent_turns to track agent turns for chat sessions
CREATE TABLE chat_coding_agent_turns (
    id                          BLOB PRIMARY KEY,
    chat_execution_process_id   BLOB NOT NULL,
    agent_session_id            TEXT,
    message_id                  TEXT,
    prompt                      TEXT,
    summary                     TEXT,
    created_at                  TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    updated_at                  TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    FOREIGN KEY (chat_execution_process_id) REFERENCES chat_execution_processes(id) ON DELETE CASCADE
);

CREATE INDEX idx_chat_coding_agent_turns_process_id ON chat_coding_agent_turns(chat_execution_process_id);
CREATE INDEX idx_chat_coding_agent_turns_session_id ON chat_coding_agent_turns(agent_session_id);
