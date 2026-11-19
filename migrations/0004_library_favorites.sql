-- EMPETO — biblioteca aprimorada, favoritos, descrições (documento, 4.1/4.2/4.5).
ALTER TABLE exercise ADD COLUMN description TEXT NOT NULL DEFAULT '';
ALTER TABLE exercise ADD COLUMN notes TEXT NOT NULL DEFAULT '';
ALTER TABLE exercise ADD COLUMN is_favorite INTEGER NOT NULL DEFAULT 0;
ALTER TABLE exercise ADD COLUMN is_custom INTEGER NOT NULL DEFAULT 1;
-- Exercícios da biblioteca inicial (semente) não são personalizados.
UPDATE exercise SET is_custom = 0 WHERE created_at = 0;
ALTER TABLE workout_plan ADD COLUMN is_favorite INTEGER NOT NULL DEFAULT 0;
CREATE INDEX ix_exercise_filters ON exercise(archived, muscle_focus, equipment, exercise_type);
