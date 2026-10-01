-- Active users
SELECT id, name
FROM users
WHERE active = TRUE AND age > 18
ORDER BY name;
