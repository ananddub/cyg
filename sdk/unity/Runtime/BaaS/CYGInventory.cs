using System;
using System.Collections.Generic;
using System.Threading.Tasks;
using UnityEngine;

namespace CookYourGame
{
    [Serializable]
    public class CYGInventoryItem
    {
        public string instance_id;
        public string user_id;
        public string item_id;
        public int quantity;
        public int slot_index;
        public string metadata;
        public string created_at;
        public string updated_at;
    }

    [Serializable]
    internal class ListInventoryResponse
    {
        public List<CYGInventoryItem> items = new List<CYGInventoryItem>();
    }

    [Serializable]
    internal class EquipItemPayload
    {
        public string instance_id;
        public int slot_index;
    }

    [Serializable]
    internal class ConsumeItemPayload
    {
        public string instance_id;
        public int quantity;
    }

    [Serializable]
    internal class GrantItemPayload
    {
        public string item_id;
        public int quantity;
        public int slot_index;
    }

    [Serializable]
    public class CYGInventoryOpResult
    {
        public bool success;
        public string message;
        public CYGInventoryItem item;
    }

    /// <summary>
    /// Server-authoritative inventory, equipment slots, and item consumption for CookYourGame.
    /// Synchronizes player inventory directly with PostgreSQL / OrioleDB backends.
    /// </summary>
    public static class CYGInventory
    {
        private static readonly List<CYGInventoryItem> cachedItems = new List<CYGInventoryItem>();

        public static IReadOnlyList<CYGInventoryItem> Items => cachedItems;

        public static event Action<List<CYGInventoryItem>> OnInventoryChanged;

        /// <summary>
        /// Fetches the authenticated player's full inventory from the server.
        /// </summary>
        public static async Task<List<CYGInventoryItem>> GetInventoryAsync()
        {
            var response = await CYGHttp.GetAsync("/v2/inventory");
            if (!response.IsSuccess)
            {
                Debug.LogError($"[CYGInventory] Failed to fetch inventory: {response.StatusCode} - {response.Text}");
                return cachedItems;
            }

            var result = JsonUtility.FromJson<ListInventoryResponse>(response.Text);
            cachedItems.Clear();
            if (result != null && result.items != null)
            {
                cachedItems.AddRange(result.items);
            }

            OnInventoryChanged?.Invoke(cachedItems);
            return cachedItems;
        }

        /// <summary>
        /// Equips or moves an item instance into a specific slot (e.g. 0 = MainHand, 1 = Helmet, etc.).
        /// </summary>
        public static async Task<bool> EquipItemAsync(string instanceId, int slotIndex)
        {
            var payload = new EquipItemPayload
            {
                instance_id = instanceId,
                slot_index = slotIndex
            };

            string body = JsonUtility.ToJson(payload);
            var response = await CYGHttp.PostAsync("/v2/inventory/equip", body);

            if (!response.IsSuccess)
            {
                Debug.LogError($"[CYGInventory] Equip failed: {response.StatusCode} - {response.Text}");
                return false;
            }

            var opResult = JsonUtility.FromJson<CYGInventoryOpResult>(response.Text);
            if (opResult != null && opResult.success)
            {
                // Refresh local cache
                await GetInventoryAsync();
                return true;
            }

            return false;
        }

        /// <summary>
        /// Consumes a quantity of an item instance (e.g. potions, ammo, scrolls).
        /// </summary>
        public static async Task<bool> ConsumeItemAsync(string instanceId, int quantity = 1)
        {
            var payload = new ConsumeItemPayload
            {
                instance_id = instanceId,
                quantity = quantity
            };

            string body = JsonUtility.ToJson(payload);
            var response = await CYGHttp.PostAsync("/v2/inventory/consume", body);

            if (!response.IsSuccess)
            {
                Debug.LogError($"[CYGInventory] Consume failed: {response.StatusCode} - {response.Text}");
                return false;
            }

            var opResult = JsonUtility.FromJson<CYGInventoryOpResult>(response.Text);
            if (opResult != null && opResult.success)
            {
                await GetInventoryAsync();
                return true;
            }

            return false;
        }

        /// <summary>
        /// Grants an item to the current player (useful for gameplay rewards or testing).
        /// </summary>
        public static async Task<bool> GrantItemAsync(string itemId, int quantity = 1, int slotIndex = -1)
        {
            var payload = new GrantItemPayload
            {
                item_id = itemId,
                quantity = quantity,
                slot_index = slotIndex
            };

            string body = JsonUtility.ToJson(payload);
            var response = await CYGHttp.PostAsync("/v2/inventory/grant", body);

            if (!response.IsSuccess)
            {
                Debug.LogError($"[CYGInventory] Grant failed: {response.StatusCode} - {response.Text}");
                return false;
            }

            var opResult = JsonUtility.FromJson<CYGInventoryOpResult>(response.Text);
            if (opResult != null && opResult.success)
            {
                await GetInventoryAsync();
                return true;
            }

            return false;
        }

        /// <summary>
        /// Finds the item assigned to a specific equipment slot, or null if empty.
        /// </summary>
        public static CYGInventoryItem GetItemInSlot(int slotIndex)
        {
            return cachedItems.Find(item => item.slot_index == slotIndex);
        }

        /// <summary>
        /// Finds an item by its unique instance UUID.
        /// </summary>
        public static CYGInventoryItem GetItemByInstanceId(string instanceId)
        {
            return cachedItems.Find(item => item.instance_id == instanceId);
        }
    }
}
