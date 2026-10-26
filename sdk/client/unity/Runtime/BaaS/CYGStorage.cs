using System;
using System.Collections.Generic;
using System.Threading.Tasks;
using UnityEngine;
using UnityEngine.Networking;

namespace CookYourGame
{
    [Serializable]
    public class CYGStorageObject
    {
        public string collection;
        public string key;
        public string user_id;
        public string value;
        public string version;
        public int permission_read;
        public int permission_write;
        public string create_time;
        public string update_time;
    }

    [Serializable]
    internal class StorageObjectListResponse
    {
        public List<CYGStorageObject> objects = new List<CYGStorageObject>();
        public string cursor;
    }

    [Serializable]
    internal class StorageObjectItemPayload
    {
        public string collection;
        public string key;
        public string value;
        public int permission_read;
        public int permission_write;
    }

    [Serializable]
    internal class StorageWritePayload
    {
        public List<StorageObjectItemPayload> objects = new List<StorageObjectItemPayload>();
    }

    [Serializable]
    internal class StorageIdPayload
    {
        public string collection;
        public string key;
        public string user_id;
    }

    [Serializable]
    internal class StorageReadPayload
    {
        public List<StorageIdPayload> object_ids = new List<StorageIdPayload>();
    }

    /// <summary>
    /// Key-Value Document Storage API for CookYourGame.
    /// Allows persisting game saves, player progression, player statistics, and game world chunks.
    /// </summary>
    public static class CYGStorage
    {
        /// <summary>
        /// Reads a document for the current authenticated player.
        /// Returns the JSON string value, or null if not found.
        /// </summary>
        public static async Task<string> ReadAsync(string collection, string key, string userId = null)
        {
            if (string.IsNullOrEmpty(userId))
            {
                userId = CYGAuth.UserId;
            }

            var payload = new StorageReadPayload();
            payload.object_ids.Add(new StorageIdPayload
            {
                collection = collection,
                key = key,
                user_id = userId
            });

            string body = JsonUtility.ToJson(payload);
            var response = await CYGHttp.PostAsync("/v2/storage", body);

            if (!response.IsSuccess)
            {
                return null;
            }

            var list = JsonUtility.FromJson<StorageObjectListResponse>(response.Text);
            if (list != null && list.objects != null && list.objects.Count > 0)
            {
                return list.objects[0].value;
            }

            return null;
        }

        /// <summary>
        /// Writes or updates a document in a collection.
        /// </summary>
        public static async Task<bool> WriteAsync(string collection, string key, string valueJson, int permissionRead = 1, int permissionWrite = 1)
        {
            var payload = new StorageWritePayload();
            payload.objects.Add(new StorageObjectItemPayload
            {
                collection = collection,
                key = key,
                value = valueJson,
                permission_read = permissionRead,
                permission_write = permissionWrite
            });

            string body = JsonUtility.ToJson(payload);
            var response = await CYGHttp.PutAsync("/v2/storage", body);

            if (!response.IsSuccess)
            {
                Debug.LogError($"[CYGStorage] Write failed: {response.StatusCode} - {response.Text}");
                return false;
            }

            return true;
        }

        /// <summary>
        /// Deletes a document by collection and key.
        /// </summary>
        public static async Task<bool> DeleteAsync(string collection, string key)
        {
            var payload = new StorageReadPayload();
            payload.object_ids.Add(new StorageIdPayload
            {
                collection = collection,
                key = key
            });

            string body = JsonUtility.ToJson(payload);
            var response = await CYGHttp.PutAsync("/v2/storage/delete", body);

            return response.IsSuccess;
        }

        /// <summary>
        /// Lists all documents in a collection.
        /// </summary>
        public static async Task<List<CYGStorageObject>> ListCollectionAsync(string collection, int limit = 100)
        {
            string path = $"/v2/storage/{UnityWebRequest.EscapeURL(collection)}?limit={limit}";
            var response = await CYGHttp.GetAsync(path);

            if (!response.IsSuccess)
            {
                Debug.LogError($"[CYGStorage] List failed: {response.StatusCode} - {response.Text}");
                return new List<CYGStorageObject>();
            }

            var result = JsonUtility.FromJson<StorageObjectListResponse>(response.Text);
            return result?.objects ?? new List<CYGStorageObject>();
        }
    }
}
