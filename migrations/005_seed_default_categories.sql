INSERT INTO categories (id, name, position, created_at)
SELECT lower(
    hex(randomblob(4)) || '-' ||
    hex(randomblob(2)) || '-' ||
    hex(randomblob(2)) || '-' ||
    hex(randomblob(2)) || '-' ||
    hex(randomblob(6))
), 'TODO', 0, strftime('%Y-%m-%dT%H:%M:%fZ','now')
WHERE NOT EXISTS (SELECT 1 FROM categories WHERE name = 'TODO');

INSERT INTO categories (id, name, position, created_at)
SELECT lower(
    hex(randomblob(4)) || '-' ||
    hex(randomblob(2)) || '-' ||
    hex(randomblob(2)) || '-' ||
    hex(randomblob(2)) || '-' ||
    hex(randomblob(6))
), 'IN PROGRESS', 1, strftime('%Y-%m-%dT%H:%M:%fZ','now')
WHERE NOT EXISTS (SELECT 1 FROM categories WHERE name = 'IN PROGRESS');

INSERT INTO categories (id, name, position, created_at)
SELECT lower(
    hex(randomblob(4)) || '-' ||
    hex(randomblob(2)) || '-' ||
    hex(randomblob(2)) || '-' ||
    hex(randomblob(2)) || '-' ||
    hex(randomblob(6))
), 'DONE', 2, strftime('%Y-%m-%dT%H:%M:%fZ','now')
WHERE NOT EXISTS (SELECT 1 FROM categories WHERE name = 'DONE');
