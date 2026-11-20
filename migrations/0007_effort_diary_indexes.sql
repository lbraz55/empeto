-- EMPETO — análise e evolução (documento de evolução v1.1, 8.4, 8.9 e 12.1).
-- Percepção de esforço por série (opcional): RPE de 1 a 10 (meios pontos) e RIR (repetições em reserva).
ALTER TABLE session_set ADD COLUMN rpe REAL CHECK (rpe IS NULL OR (rpe >= 1 AND rpe <= 10));
ALTER TABLE session_set ADD COLUMN rir INTEGER CHECK (rir IS NULL OR rir BETWEEN 0 AND 10);
-- Diário: como foi o treino (1 = muito ruim … 5 = excelente), junto das observações da sessão.
ALTER TABLE workout_session ADD COLUMN feeling INTEGER CHECK (feeling IS NULL OR feeling BETWEEN 1 AND 5);
-- O histórico continua imutável: a avaliação também não muda depois de concluída a sessão.
CREATE TRIGGER trg_session_feeling_immutable
BEFORE UPDATE OF feeling ON workout_session
WHEN OLD.status = 'completed' AND NEW.feeling IS NOT OLD.feeling
BEGIN
    SELECT RAISE(ABORT, 'historico imutavel: sessao concluida');
END;
-- Índices para relatórios e comparação (consultas por período e por exercício).
CREATE INDEX IF NOT EXISTS ix_session_status_started ON workout_session(status, started_at);
CREATE INDEX IF NOT EXISTS ix_session_exercise_performed ON session_exercise(performed_exercise_id);
