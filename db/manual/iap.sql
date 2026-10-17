-- Agent: IAP. Statements FlashORM cannot generate for the routes added with the
-- Facebook Instant / Samsung vendor parity and the console product-level delete.
-- Mirrored in `crates/engine-api/src/services/iap/subscriptions.rs` and
-- `crates/engine-api/src/services/console/iap/maintenance.rs`.

-- name: FindSubscriptionByProduct :one
-- The player's most recent subscription for one product id (GetSubscription
-- parity); the generated GetSubscription is keyed by original transaction id.
SELECT * FROM subscription
WHERE user_id = $1 AND product_id = $2
ORDER BY purchase_time DESC, original_transaction_id ASC LIMIT 1;

-- name: DeleteSubscriptionsByProduct :exec
-- The console deletes the subscription entity for a product; the schema carries
-- no purchase-to-subscription link, so purchase rows are deliberately untouched.
DELETE FROM subscription WHERE product_id = $1;
