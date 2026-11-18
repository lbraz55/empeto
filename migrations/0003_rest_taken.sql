-- Descanso realmente cumprido após a série (segundos), medido pelo cronômetro.
-- NULL = não medido (cronômetro não usado após esta série).
ALTER TABLE session_set ADD COLUMN rest_taken_s INTEGER
    CHECK (rest_taken_s IS NULL OR rest_taken_s BETWEEN 0 AND 10800);
