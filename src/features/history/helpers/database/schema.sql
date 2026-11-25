CREATE TABLE runs (
    id INTEGER PRIMARY KEY,
    model_label TEXT NOT NULL CHECK (length(model_label) > 0),
    started_at INTEGER NOT NULL,
    finished_at INTEGER NOT NULL CHECK (finished_at >= started_at),
    exit_code INTEGER NOT NULL CHECK (exit_code IN (0, 1, 2)),
    linter_version TEXT NOT NULL,
    policy_fingerprint TEXT NOT NULL,
    baseline_run_id INTEGER REFERENCES runs (id),
    failure_stage TEXT,
    CHECK ((exit_code = 2) = (failure_stage IS NOT NULL))
) STRICT;

CREATE TABLE findings (
    id INTEGER PRIMARY KEY,
    run_id INTEGER NOT NULL REFERENCES runs (id) ON DELETE CASCADE,
    rule_id TEXT NOT NULL,
    severity TEXT NOT NULL CHECK (severity IN ('error', 'warning'))
) STRICT;

CREATE TABLE finding_locations (
    finding_id INTEGER NOT NULL REFERENCES findings (id) ON DELETE CASCADE,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    path TEXT NOT NULL,
    line INTEGER CHECK (line > 0),
    column INTEGER CHECK (column > 0),
    PRIMARY KEY (finding_id, ordinal)
) STRICT;

CREATE INDEX runs_finished ON runs (finished_at);
CREATE INDEX runs_model_finished ON runs (model_label, finished_at);
CREATE INDEX findings_run ON findings (run_id);
