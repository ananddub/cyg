use super::*;

impl Queries {
    pub async fn list_inventory(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<PlayerInventoryItem>, DbError> {
        self.all(
            "SELECT * FROM player_inventory_item WHERE user_id = $user_id \
             ORDER BY slot_index ASC, created_at ASC",
            params!(user_id = user_id.to_string()),
        )
        .await
    }

    pub async fn get_inventory_item(
        &self,
        instance_id: Uuid,
    ) -> Result<Option<PlayerInventoryItem>, DbError> {
        self.opt(
            "SELECT * FROM type::record('player_inventory_item', <uuid>$instance_id) LIMIT 1",
            params!(instance_id = instance_id.to_string()),
        )
        .await
    }

    pub async fn get_item_definition(
        &self,
        item_id: &str,
    ) -> Result<Option<ItemDefinition>, DbError> {
        self.opt(
            "SELECT * FROM type::record('item_definition', $item_id) LIMIT 1",
            params!(item_id = item_id),
        )
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
    ) -> Result<(), DbError> {
        let created_at = match self.get_item_definition(item_id).await? {
            Some(row) => row.created_at,
            None => Utc::now(),
        };
        let row = ItemDefinition {
            item_id: item_id.to_string(),
            name: name.to_string(),
            category: category.to_string(),
            max_stack,
            rarity: rarity.to_string(),
            properties: properties.clone(),
            created_at,
        };
        self.exec(
            "UPSERT type::record('item_definition', $item_id) CONTENT $data",
            params!(item_id = item_id, data = row),
        )
        .await
    }

    pub async fn grant_inventory_item(
        &self,
        instance_id: Uuid,
        user_id: Uuid,
        item_id: &str,
        quantity: i32,
        slot_index: i32,
        metadata: &serde_json::Value,
    ) -> Result<PlayerInventoryItem, DbError> {
        let now = Utc::now();
        let row = PlayerInventoryItem {
            instance_id,
            user_id,
            item_id: item_id.to_string(),
            quantity,
            slot_index,
            metadata: metadata.clone(),
            created_at: now,
            updated_at: now,
        };
        self.create("player_inventory_item", Key::Uuid(instance_id), &row)
            .await
    }

    pub async fn update_inventory_quantity(
        &self,
        instance_id: Uuid,
        quantity: i32,
    ) -> Result<(), DbError> {
        if quantity <= 0 {
            self.exec(
                "DELETE type::record('player_inventory_item', <uuid>$instance_id)",
                params!(instance_id = instance_id.to_string()),
            )
            .await
        } else {
            self.exec(
                "UPDATE type::record('player_inventory_item', <uuid>$instance_id) \
                 SET quantity = $quantity, updated_at = $updated_at",
                params!(
                    instance_id = instance_id.to_string(),
                    quantity = quantity,
                    updated_at = ts(Utc::now()),
                ),
            )
            .await
        }
    }

    pub async fn set_inventory_slot(
        &self,
        instance_id: Uuid,
        slot_index: i32,
    ) -> Result<(), DbError> {
        self.exec(
            "UPDATE type::record('player_inventory_item', <uuid>$instance_id) \
             SET slot_index = $slot_index, updated_at = $updated_at",
            params!(
                instance_id = instance_id.to_string(),
                slot_index = slot_index,
                updated_at = ts(Utc::now()),
            ),
        )
        .await
    }

    pub async fn save_room_snapshot(
        &self,
        room_name: &str,
        backend: &str,
        snapshot_bytes: &[u8],
        entity_count: i32,
    ) -> Result<(), DbError> {
        let row = RoomPersistentSnapshot {
            room_name: room_name.to_string(),
            backend: backend.to_string(),
            snapshot_bytes: snapshot_bytes.to_vec(),
            entity_count,
            saved_at: Utc::now(),
        };
        self.exec(
            "UPSERT type::record('room_persistent_snapshot', $room_name) CONTENT $data",
            params!(room_name = room_name, data = row),
        )
        .await
    }

    pub async fn load_room_snapshot(
        &self,
        room_name: &str,
    ) -> Result<Option<RoomPersistentSnapshot>, DbError> {
        self.opt(
            "SELECT * FROM type::record('room_persistent_snapshot', $room_name) LIMIT 1",
            params!(room_name = room_name),
        )
        .await
    }
}
