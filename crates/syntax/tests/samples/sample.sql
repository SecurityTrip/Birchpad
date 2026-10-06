-- Active users
SELECT id, name
FROM users
WHERE active = TRUE AND age > 18 AND name LIKE 'A%'
ORDER BY name;

CREATE TABLE prices (
    id INTEGER PRIMARY KEY,
    amount DECIMAL(10, 2) DEFAULT 1.5
);
