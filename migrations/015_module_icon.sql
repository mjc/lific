-- LIF-124: modules get an optional icon/emoji, same as projects.
-- Stores either a "lucide:<Name>" reference or a literal emoji char,
-- mirroring projects.emoji. NULL = no icon (UI falls back to a default).
ALTER TABLE modules ADD COLUMN emoji TEXT;
