-- Agent 2: IAP purchases, subscriptions and the wallet ledger. BaaS parity:
-- server/core_purchase.go — a repeat receipt is not an error, it is an update,
-- and "seen before" is derived from update_time being later than create_time.

-- name: InsertPurchase :one
INSERT INTO purchase (transaction_id, user_id, store, environment, product_id, purchase_time, refund_time, raw_response)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
ON CONFLICT (transaction_id) DO UPDATE
SET user_id = EXCLUDED.user_id, product_id = EXCLUDED.product_id,
    purchase_time = EXCLUDED.purchase_time, refund_time = EXCLUDED.refund_time,
    raw_response = EXCLUDED.raw_response, update_time = now()
RETURNING *;

-- name: GetPurchase :one
SELECT * FROM purchase WHERE transaction_id = $1;

-- name: ListPurchases :many
SELECT * FROM purchase
WHERE user_id = $1 AND refund_time = '1970-01-01 00:00:00+00'
ORDER BY purchase_time DESC, transaction_id ASC LIMIT $2 OFFSET $3;

-- name: InsertSubscription :one
INSERT INTO subscription (original_transaction_id, user_id, store, environment, product_id, purchase_time, expire_time, refund_time, raw_response, raw_notification)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, '{}'::jsonb)
ON CONFLICT (original_transaction_id) DO UPDATE
SET user_id = EXCLUDED.user_id, product_id = EXCLUDED.product_id,
    purchase_time = EXCLUDED.purchase_time, expire_time = EXCLUDED.expire_time,
    refund_time = EXCLUDED.refund_time, raw_response = EXCLUDED.raw_response,
    raw_notification = EXCLUDED.raw_notification, update_time = now()
RETURNING *;

-- name: GetSubscription :one
SELECT * FROM subscription WHERE original_transaction_id = $1;

-- name: ListSubscriptions :many
SELECT * FROM subscription
WHERE user_id = $1 AND (expire_time > now() OR refund_time > '1970-01-01 00:00:00+00')
ORDER BY purchase_time DESC, original_transaction_id ASC LIMIT $2 OFFSET $3;

-- name: ListAllSubscriptions :many
SELECT * FROM subscription
ORDER BY purchase_time DESC, original_transaction_id ASC LIMIT $1 OFFSET $2;

-- name: UpdateUserWallet :one
UPDATE users SET wallet = $2, update_time = now() WHERE id = $1
RETURNING wallet;

-- name: InsertWalletLedger :exec
INSERT INTO wallet_ledger (id, user_id, changeset, metadata)
VALUES ($1, $2, $3, $4);
