-- EMPETO — eficiência de treinamento (documento de evolução v1.1, 8.1 e 8.2).
-- Tipo de cada série: planejado na ficha (lista por série) e registrado na sessão/histórico.
-- Séries antigas recebem o tipo 'normal'.
ALTER TABLE plan_exercise ADD COLUMN set_kinds TEXT NOT NULL DEFAULT '';
ALTER TABLE session_set ADD COLUMN set_kind TEXT NOT NULL DEFAULT 'normal'
    CHECK (set_kind IN ('normal', 'warmup', 'drop', 'failure', 'backoff', 'technique'));
-- Grupos (supersérie/trissérie/circuito): exercícios consecutivos com a mesma etiqueta.
-- Cada exercício continua com seus próprios registros; a etiqueta é copiada para a sessão.
ALTER TABLE plan_exercise ADD COLUMN group_tag TEXT NOT NULL DEFAULT '';
ALTER TABLE session_exercise ADD COLUMN group_tag TEXT NOT NULL DEFAULT '';
