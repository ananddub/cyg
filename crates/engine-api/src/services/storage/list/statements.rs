//! One statement per listing shape and page direction. The three shapes order
//! differently, so each has its own pair and its own cursor comparison.

pub(super) const PUBLIC_ALL_FIRST: &str = "
SELECT collection, key, user_id, value::text AS value, version, read, write, create_time, update_time
FROM storage WHERE collection = $1 AND read >= 2
ORDER BY read ASC, key ASC, user_id ASC LIMIT $2";

pub(super) const PUBLIC_ALL_NEXT: &str = "
SELECT collection, key, user_id, value::text AS value, version, read, write, create_time, update_time
FROM storage
WHERE collection = $1 AND read >= 2 AND (read, key, user_id) > (2, $2, $3)
ORDER BY read ASC, key ASC, user_id ASC LIMIT $4";

pub(super) const OWN_FIRST: &str = "
SELECT collection, key, user_id, value::text AS value, version, read, write, create_time, update_time
FROM storage WHERE collection = $1 AND user_id = $2 AND read >= 1
ORDER BY read ASC, key ASC LIMIT $3";

pub(super) const OWN_NEXT: &str = "
SELECT collection, key, user_id, value::text AS value, version, read, write, create_time, update_time
FROM storage
WHERE collection = $1 AND user_id = $2 AND read >= 1 AND (read, key) > ($3, $4)
ORDER BY read ASC, key ASC LIMIT $5";

pub(super) const PUBLIC_USER_FIRST: &str = "
SELECT collection, key, user_id, value::text AS value, version, read, write, create_time, update_time
FROM storage WHERE collection = $1 AND user_id = $2 AND read = 2
ORDER BY key ASC LIMIT $3";

pub(super) const PUBLIC_USER_NEXT: &str = "
SELECT collection, key, user_id, value::text AS value, version, read, write, create_time, update_time
FROM storage
WHERE collection = $1 AND user_id = $2 AND read = 2 AND key > $3
ORDER BY key ASC LIMIT $4";
