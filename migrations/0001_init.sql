-- WORKOUT — schema v1
-- Datas: epoch em milissegundos (UTC). Cargas: inteiro em gramas (sem ponto flutuante).

CREATE TABLE exercise (
    id               INTEGER PRIMARY KEY,
    name             TEXT    NOT NULL CHECK (length(trim(name)) > 0),
    muscle_focus     TEXT    NOT NULL DEFAULT '',
    exercise_type    TEXT    NOT NULL DEFAULT '',
    equipment        TEXT    NOT NULL DEFAULT '',
    movement_pattern TEXT    NOT NULL DEFAULT '',
    archived         INTEGER NOT NULL DEFAULT 0,
    created_at       INTEGER NOT NULL,
    updated_at       INTEGER NOT NULL
);
CREATE UNIQUE INDEX ux_exercise_name ON exercise(name COLLATE NOCASE);

CREATE TABLE workout_plan (
    id            INTEGER PRIMARY KEY,
    name          TEXT    NOT NULL CHECK (length(trim(name)) > 0),
    description   TEXT    NOT NULL DEFAULT '',
    schedule_type TEXT    NOT NULL CHECK (schedule_type IN ('weekday', 'sequence')),
    active        INTEGER NOT NULL DEFAULT 1,
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL
);

-- Dia da ficha: por dias da semana (bitmask seg=1 .. dom=64) e/ou ordem na sequência A/B/C.
CREATE TABLE plan_schedule (
    id             INTEGER PRIMARY KEY,
    plan_id        INTEGER NOT NULL REFERENCES workout_plan(id) ON DELETE CASCADE,
    name           TEXT    NOT NULL CHECK (length(trim(name)) > 0),
    weekdays       INTEGER NOT NULL DEFAULT 0 CHECK (weekdays BETWEEN 0 AND 127),
    sequence_order INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX ix_plan_schedule_plan ON plan_schedule(plan_id);

CREATE TABLE plan_exercise (
    id              INTEGER PRIMARY KEY,
    plan_id         INTEGER NOT NULL REFERENCES workout_plan(id) ON DELETE CASCADE,
    schedule_id     INTEGER NOT NULL REFERENCES plan_schedule(id) ON DELETE CASCADE,
    exercise_id     INTEGER NOT NULL REFERENCES exercise(id) ON DELETE RESTRICT,
    position        INTEGER NOT NULL,
    target_sets     INTEGER NOT NULL CHECK (target_sets BETWEEN 1 AND 50),
    target_reps_min INTEGER NOT NULL CHECK (target_reps_min BETWEEN 0 AND 1000),
    target_reps_max INTEGER NOT NULL CHECK (target_reps_max BETWEEN target_reps_min AND 1000),
    target_load_g   INTEGER NOT NULL DEFAULT 0 CHECK (target_load_g BETWEEN 0 AND 2000000),
    rest_seconds    INTEGER NOT NULL DEFAULT 90 CHECK (rest_seconds BETWEEN 0 AND 3600),
    notes           TEXT    NOT NULL DEFAULT ''
);
CREATE INDEX ix_plan_exercise_schedule ON plan_exercise(schedule_id, position);

-- Sessão realizada: registro histórico. plan_id/schedule_id podem virar NULL se a ficha for
-- excluída (RN-08); todo o restante fica congelado após a conclusão (RN-01).
CREATE TABLE workout_session (
    id                 INTEGER PRIMARY KEY,
    plan_id            INTEGER REFERENCES workout_plan(id) ON DELETE SET NULL,
    schedule_id        INTEGER REFERENCES plan_schedule(id) ON DELETE SET NULL,
    plan_name          TEXT    NOT NULL,
    schedule_name      TEXT    NOT NULL,
    started_at         INTEGER NOT NULL,
    ended_at           INTEGER,
    status             TEXT    NOT NULL CHECK (status IN ('in_progress', 'completed')),
    notes              TEXT    NOT NULL DEFAULT '',
    plan_snapshot_json TEXT    NOT NULL
);
CREATE INDEX ix_session_started ON workout_session(started_at);
CREATE UNIQUE INDEX ux_single_active_session ON workout_session(status) WHERE status = 'in_progress';

CREATE TABLE session_exercise (
    id                    INTEGER PRIMARY KEY,
    session_id            INTEGER NOT NULL REFERENCES workout_session(id) ON DELETE CASCADE,
    plan_exercise_id      INTEGER REFERENCES plan_exercise(id) ON DELETE SET NULL,
    original_exercise_id  INTEGER REFERENCES exercise(id) ON DELETE SET NULL,
    performed_exercise_id INTEGER REFERENCES exercise(id) ON DELETE SET NULL,
    original_name         TEXT    NOT NULL,
    performed_name        TEXT    NOT NULL,
    muscle_focus          TEXT    NOT NULL DEFAULT '',
    exercise_type         TEXT    NOT NULL DEFAULT '',
    equipment             TEXT    NOT NULL DEFAULT '',
    substitution_type     TEXT    NOT NULL DEFAULT 'none'
                                  CHECK (substitution_type IN ('none', 'temporary', 'permanent')),
    position              INTEGER NOT NULL,
    rest_seconds          INTEGER NOT NULL DEFAULT 0,
    notes                 TEXT    NOT NULL DEFAULT '',
    completed             INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX ix_session_exercise_session ON session_exercise(session_id, position);
CREATE INDEX ix_session_exercise_performed ON session_exercise(performed_exercise_id);

CREATE TABLE session_set (
    id                  INTEGER PRIMARY KEY,
    session_exercise_id INTEGER NOT NULL REFERENCES session_exercise(id) ON DELETE CASCADE,
    set_number          INTEGER NOT NULL,
    planned_reps_min    INTEGER NOT NULL,
    planned_reps_max    INTEGER NOT NULL,
    planned_load_g      INTEGER NOT NULL,
    actual_reps         INTEGER CHECK (actual_reps IS NULL OR actual_reps BETWEEN 0 AND 1000),
    actual_load_g       INTEGER CHECK (actual_load_g IS NULL OR actual_load_g BETWEEN 0 AND 2000000),
    completed_at        INTEGER,
    UNIQUE (session_exercise_id, set_number)
);

CREATE TABLE exercise_alternative (
    id             INTEGER PRIMARY KEY,
    exercise_id    INTEGER NOT NULL REFERENCES exercise(id) ON DELETE CASCADE,
    alternative_id INTEGER NOT NULL REFERENCES exercise(id) ON DELETE CASCADE,
    reason         TEXT    NOT NULL DEFAULT '',
    UNIQUE (exercise_id, alternative_id),
    CHECK (exercise_id <> alternative_id)
);

CREATE TABLE app_preference (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

-- ---------------------------------------------------------------------------
-- Histórico imutável (RN-01 / RN-04 / RN-08): após a conclusão, a sessão e seus
-- registros não podem ser alterados. Somente a desvinculação automática de
-- referências (ON DELETE SET NULL) é permitida.
-- ---------------------------------------------------------------------------
CREATE TRIGGER trg_session_immutable
BEFORE UPDATE ON workout_session
WHEN OLD.status = 'completed' AND (
       NEW.plan_name IS NOT OLD.plan_name
    OR NEW.schedule_name IS NOT OLD.schedule_name
    OR NEW.started_at IS NOT OLD.started_at
    OR NEW.ended_at IS NOT OLD.ended_at
    OR NEW.status IS NOT OLD.status
    OR NEW.notes IS NOT OLD.notes
    OR NEW.plan_snapshot_json IS NOT OLD.plan_snapshot_json
    OR (NEW.plan_id IS NOT OLD.plan_id AND NEW.plan_id IS NOT NULL)
    OR (NEW.schedule_id IS NOT OLD.schedule_id AND NEW.schedule_id IS NOT NULL))
BEGIN
    SELECT RAISE(ABORT, 'historico imutavel: sessao concluida');
END;

CREATE TRIGGER trg_session_delete_immutable
BEFORE DELETE ON workout_session
WHEN OLD.status = 'completed'
BEGIN
    SELECT RAISE(ABORT, 'historico imutavel: sessao concluida');
END;

CREATE TRIGGER trg_session_exercise_immutable
BEFORE UPDATE ON session_exercise
WHEN (SELECT status FROM workout_session WHERE id = OLD.session_id) = 'completed' AND (
       NEW.original_name IS NOT OLD.original_name
    OR NEW.performed_name IS NOT OLD.performed_name
    OR NEW.muscle_focus IS NOT OLD.muscle_focus
    OR NEW.exercise_type IS NOT OLD.exercise_type
    OR NEW.equipment IS NOT OLD.equipment
    OR NEW.substitution_type IS NOT OLD.substitution_type
    OR NEW.position IS NOT OLD.position
    OR NEW.rest_seconds IS NOT OLD.rest_seconds
    OR NEW.notes IS NOT OLD.notes
    OR NEW.completed IS NOT OLD.completed
    OR (NEW.plan_exercise_id IS NOT OLD.plan_exercise_id AND NEW.plan_exercise_id IS NOT NULL)
    OR (NEW.original_exercise_id IS NOT OLD.original_exercise_id AND NEW.original_exercise_id IS NOT NULL)
    OR (NEW.performed_exercise_id IS NOT OLD.performed_exercise_id AND NEW.performed_exercise_id IS NOT NULL))
BEGIN
    SELECT RAISE(ABORT, 'historico imutavel: exercicio de sessao concluida');
END;

CREATE TRIGGER trg_session_exercise_insert_immutable
BEFORE INSERT ON session_exercise
WHEN (SELECT status FROM workout_session WHERE id = NEW.session_id) = 'completed'
BEGIN
    SELECT RAISE(ABORT, 'historico imutavel: sessao concluida');
END;

CREATE TRIGGER trg_session_set_immutable
BEFORE UPDATE ON session_set
WHEN (SELECT s.status FROM workout_session s JOIN session_exercise e ON e.session_id = s.id
      WHERE e.id = OLD.session_exercise_id) = 'completed'
BEGIN
    SELECT RAISE(ABORT, 'historico imutavel: serie de sessao concluida');
END;

CREATE TRIGGER trg_session_set_insert_immutable
BEFORE INSERT ON session_set
WHEN (SELECT s.status FROM workout_session s JOIN session_exercise e ON e.session_id = s.id
      WHERE e.id = NEW.session_exercise_id) = 'completed'
BEGIN
    SELECT RAISE(ABORT, 'historico imutavel: sessao concluida');
END;

CREATE TRIGGER trg_session_set_delete_immutable
BEFORE DELETE ON session_set
WHEN (SELECT s.status FROM workout_session s JOIN session_exercise e ON e.session_id = s.id
      WHERE e.id = OLD.session_exercise_id) = 'completed'
BEGIN
    SELECT RAISE(ABORT, 'historico imutavel: sessao concluida');
END;
