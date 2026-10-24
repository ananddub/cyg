//! Inventory and Room Snapshot queries.

use super::db::Queries;
use super::models::*;
use uuid::Uuid;

impl Queries {
    pub async fn list_inventory(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<PlayerInventoryItem>, sqlx::Error> {
        sqlx::query_as::<_, PlayerInventoryItem>(
            "SELECT instance_id, user_id, item_id, quantity, slot_index, metadata, created_at, updated_at \
             FROM player_inventory WHERE user_id = $1 ORDER BY slot_index ASC, created_at ASC"
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
    }

    pub async fn get_inventory_item(
        &self,
        instance_id: Uuid,
    ) -> Result<Option<PlayerInventoryItem>, sqlx::Error> {
        sqlx::query_as::<_, PlayerInventoryItem>(
            "SELECT instance_id, user_id, item_id, quantity, slot_index, metadata, created_at, updated_at \
             FROM player_inventory WHERE instance_id = $1"
        )
        .bind(instance_id)
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn get_item_definition(
        &self,
        item_id: &str,
    ) -> Result<Option<ItemDefinition>, sqlx::Error> {
        sqlx::query_as::<_, ItemDefinition>(
            "SELECT item_id, name, category, max_stack, rarity, properties, created_at \
             FROM item_definitions WHERE item_id = $1",
        )
        .bind(item_id)
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn upsert_item_definition(
        &self,
        item_id: &str,
        name: &str,
        category: &str,
        max_stack: i32,
        rarity: &str,
        properties: &serde_json::Value,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO item_definitions (item_id, name, category, max_stack, rarity, properties, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6, NOW()) \
             ON CONFLICT (item_id) DO UPDATE SET \
                name = EXCLUDED.name, \
                category = EXCLUDED.category, \
                max_stack = EXCLUDED.max_stack, \
                rarity = EXCLUDED.rarity, \
                properties = EXCLUDED.properties"
        )
        .bind(item_id)
        .bind(name)
        .bind(category)
        .bind(max_stack)
        .bind(rarity)
        .bind(properties)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn grant_inventory_item(
        &self,
        instance_id: Uuid,
        user_id: Uuid,
        item_id: &str,
        quantity: i32,
        slot_index: i32,
        metadata: &serde_json::Value,
    ) -> Result<PlayerInventoryItem, sqlx::Error> {
        sqlx::query_as::<_, PlayerInventoryItem>(
            "INSERT INTO player_inventory (instance_id, user_id, item_id, quantity, slot_index, metadata, created_at, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $6, NOW(), NOW()) \
             RETURNING instance_id, user_id, item_id, quantity, slot_index, metadata, created_at, updated_at"
        )
        .bind(instance_id)
        .bind(user_id)
        .bind(item_id)
        .bind(quantity)
        .bind(slot_index)
        .bind(metadata)
        .fetch_one(&self.pool)
        .await
    }

    pub async fn update_inventory_quantity(
        &self,
        instance_id: Uuid,
        quantity: i32,
    ) -> Result<(), sqlx::Error> {
        if quantity <= 0 {
            sqlx::query("DELETE FROM player_inventory WHERE instance_id = $1")
                .bind(instance_id)
                .execute(&self.pool)
                .await?;
        } else {
            sqlx::query("UPDATE player_inventory SET quantity = $1, updated_at = NOW() WHERE instance_id = $2")
                .bind(quantity)
                .bind(instance_id)
                .execute(&self.pool)
                .await?;
        }
        Ok(())
    }

    pub async fn set_inventory_slot(
        &self,
        instance_id: Uuid,
        slot_index: i32,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE player_inventory SET slot_index = $1, updated_at = NOW() WHERE instance_id = $2")
            .bind(slot_index)
            .bind(instance_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn save_room_snapshot(
        &self,
        room_name: &str,
        backend: &str,
        snapshot_bytes: &[u8],
        entity_count: i32,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO room_persistent_snapshots (room_name, backend, snapshot_bytes, entity_count, saved_at) \
             VALUES ($1, $2, $3, $4, NOW()) \
             ON CONFLICT (room_name) DO UPDATE SET \
                backend = EXCLUDED.backend, \
                snapshot_bytes = EXCLUDED.snapshot_bytes, \
                entity_count = EXCLUDED.entity_count, \
                saved_at = EXCLUDED.saved_at"
        )
        .bind(room_name)
        .bind(backend)
        .bind(snapshot_bytes)
        .bind(entity_count)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn load_room_snapshot(
        &self,
        room_name: &str,
    ) -> Result<Option<RoomPersistentSnapshot>, sqlx::Error> {
        sqlx::query_as::<_, RoomPersistentSnapshot>(
            "SELECT room_name, backend, snapshot_bytes, entity_count, saved_at \
             FROM room_persistent_snapshots WHERE room_name = $1",
        )
        .bind(room_name)
        .fetch_optional(&self.pool)
        .await
    }
}
