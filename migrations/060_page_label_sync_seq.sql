-- Label-only writes must advance the parent page in the sync stream.
-- Match migration 045's issue-label triggers so every writer, label merge,
-- and label deletion cascade invalidates the page's previous snapshot.
-- Stamp seq explicitly alongside updated_at to avoid another sequence bump
-- from stamp_pages_au. Ignored inserts and deletes matching no rows do not
-- fire these triggers.

CREATE TRIGGER IF NOT EXISTS page_labels_bump_ai
AFTER INSERT ON page_labels
BEGIN
    UPDATE sync_seq SET value = value + 1 WHERE id = 1;
    UPDATE pages
       SET updated_at = datetime('now'),
           seq = (SELECT value FROM sync_seq WHERE id = 1)
     WHERE id = NEW.page_id;
END;

CREATE TRIGGER IF NOT EXISTS page_labels_bump_ad
AFTER DELETE ON page_labels
BEGIN
    UPDATE sync_seq SET value = value + 1 WHERE id = 1;
    UPDATE pages
       SET updated_at = datetime('now'),
           seq = (SELECT value FROM sync_seq WHERE id = 1)
     WHERE id = OLD.page_id;
END;
