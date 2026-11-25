WITH selected AS (
    SELECT id, baseline_run_id FROM runs
    WHERE (?1 IS NULL OR model_label = ?1)
      AND (?2 IS NULL OR finished_at >= ?2)
      AND (?3 IS NULL OR id = ?3)
),
relevant AS (
    SELECT id FROM selected
    UNION
    SELECT baseline_run_id FROM selected WHERE baseline_run_id IS NOT NULL
)
SELECT r.id AS run_id, r.model_label, datetime(r.finished_at, 'unixepoch') AS finished,
       r.exit_code, r.linter_version, r.policy_fingerprint, r.baseline_run_id, r.failure_stage,
       s.id IS NOT NULL AS selected, f.id AS finding_id, f.rule_id, f.severity,
       p.path, p.line, p.column
FROM relevant
JOIN runs r ON r.id = relevant.id
LEFT JOIN selected s ON s.id = r.id
LEFT JOIN findings f ON f.run_id = r.id
LEFT JOIN finding_locations p ON p.finding_id = f.id
ORDER BY r.id, f.id, p.ordinal
