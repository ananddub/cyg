using System;
using System.Collections.Generic;
using System.Threading.Tasks;
using UnityEngine;
using UnityEngine.Networking;

namespace CookYourGame
{
    [Serializable]
    public class CYGChatMessage
    {
        public string channel_id, message_id, sender_id, username, content;
        public string create_time, update_time, room_name, group_id, user_id_one, user_id_two;
        public int code;
        public bool persistent;

        /// <summary>The `text` member of content, or the raw content for non-text payloads.</summary>
        public string Text => CYGChat.ExtractText(content);
    }

    [Serializable]
    public class CYGChatMessageList
    {
        public List<CYGChatMessage> messages = new List<CYGChatMessage>();
        public string next_cursor, prev_cursor, cacheable_cursor;
    }

    [Serializable]
    internal class CYGChatTextContent
    {
        public string text;
    }

    /// <summary>
    /// Text channels over the control plane's `/v2/channel` routes, with a local
    /// timeline cache. Reads and writes are Task-based like the other BaaS
    /// clients; the events fire on the main thread after a successful write or
    /// ingest. A channel id is the server's dotted four-part id
    /// (`<mode>.<subject>.<descriptor>.<label>`); use RoomChannelId,
    /// GroupChannelId or DirectChannelId to build one from the ids you have.
    /// </summary>
    public static class CYGChat
    {
        public const int CacheLimit = 500;

        private static readonly Dictionary<string, List<CYGChatMessage>> Cache =
            new Dictionary<string, List<CYGChatMessage>>();

        public static event Action<CYGChatMessage> MessageReceived;
        public static event Action<string, IReadOnlyList<CYGChatMessage>> MessagesUpdated;

        /// <summary>Reads a page of stored history and merges it into the cache.</summary>
        public static async Task<CYGChatMessageList> HistoryAsync(
            string channelId, int limit = 0, string cursor = null, bool forward = true)
        {
            if (string.IsNullOrEmpty(channelId))
            {
                Debug.LogError("[CYGChat] A channel id is required.");
                return null;
            }

            string path = $"/v2/channel/{UnityWebRequest.EscapeURL(channelId)}";
            var query = new List<string>();
            if (limit > 0) query.Add($"limit={limit}");
            if (!string.IsNullOrEmpty(cursor)) query.Add($"cursor={UnityWebRequest.EscapeURL(cursor)}");
            if (!forward) query.Add("forward=false");
            if (query.Count > 0) path += "?" + string.Join("&", query);

            var response = await CYGHttp.GetAsync(path);
            if (!response.IsSuccess)
            {
                LogFailure("Read history", response);
                return null;
            }

            var page = Parse<CYGChatMessageList>(response.Text, "history page");
            if (page == null) return null;
            if (page.messages == null) page.messages = new List<CYGChatMessage>();

            foreach (var message in page.messages) Merge(channelId, message);
            NotifyUpdated(channelId);
            return page;
        }

        /// <summary>Sends plain text, wrapped as `{"text": ...}` by the wire contract.</summary>
        public static Task<CYGChatMessage> SendAsync(string channelId, string text)
        {
            return SendContentAsync(channelId, JsonUtility.ToJson(new CYGChatTextContent { text = text }));
        }

        /// <summary>Sends a raw JSON object or array as the message content.</summary>
        public static Task<CYGChatMessage> SendContentAsync(string channelId, string contentJson)
        {
            if (string.IsNullOrEmpty(channelId) || string.IsNullOrEmpty(contentJson))
            {
                Debug.LogError("[CYGChat] A channel id and content are required.");
                return Task.FromResult<CYGChatMessage>(null);
            }

            string path = $"/v2/channel/{UnityWebRequest.EscapeURL(channelId)}/message";
            return WriteAsync(path, contentJson, channelId, false);
        }

        /// <summary>Replaces a message's content; only the content changes.</summary>
        public static Task<CYGChatMessage> EditAsync(string channelId, string messageId, string text)
        {
            return EditContentAsync(channelId, messageId,
                JsonUtility.ToJson(new CYGChatTextContent { text = text }));
        }

        /// <summary>Replaces a message's content with a raw JSON object or array.</summary>
        public static Task<CYGChatMessage> EditContentAsync(string channelId, string messageId, string contentJson)
        {
            if (string.IsNullOrEmpty(channelId) || string.IsNullOrEmpty(messageId) || string.IsNullOrEmpty(contentJson))
            {
                Debug.LogError("[CYGChat] A channel id, message id and content are required.");
                return Task.FromResult<CYGChatMessage>(null);
            }

            string path = $"/v2/channel/{UnityWebRequest.EscapeURL(channelId)}/message/{UnityWebRequest.EscapeURL(messageId)}";
            return WriteAsync(path, contentJson, channelId, true);
        }

        /// <summary>Deletes a message and drops it from the cache.</summary>
        public static async Task<CYGChatMessage> RemoveAsync(string channelId, string messageId)
        {
            if (string.IsNullOrEmpty(channelId) || string.IsNullOrEmpty(messageId))
            {
                Debug.LogError("[CYGChat] A channel id and message id are required.");
                return null;
            }

            string path = $"/v2/channel/{UnityWebRequest.EscapeURL(channelId)}/message/{UnityWebRequest.EscapeURL(messageId)}";
            var response = await CYGHttp.DeleteAsync(path);
            if (!response.IsSuccess)
            {
                LogFailure("Remove message", response);
                return null;
            }

            var message = Parse<CYGChatMessage>(response.Text, "message");
            if (message == null) return null;

            RemoveCached(channelId, string.IsNullOrEmpty(message.message_id) ? messageId : message.message_id);
            NotifyUpdated(channelId);
            return message;
        }

        /// <summary>Feeds a live `apiChannelMessage` JSON payload into the cache and events.</summary>
        public static bool IngestMessage(string messageJson)
        {
            var message = Parse<CYGChatMessage>(messageJson, "message");
            return message != null && IngestMessage(message);
        }

        /// <summary>Feeds an already-parsed message into the cache and events.</summary>
        public static bool IngestMessage(CYGChatMessage message)
        {
            if (message == null || string.IsNullOrEmpty(message.channel_id)) return false;

            Merge(message.channel_id, message);
            MessageReceived?.Invoke(message);
            NotifyUpdated(message.channel_id);
            return true;
        }

        /// <summary>A copy of the cached timeline for a channel, oldest first.</summary>
        public static List<CYGChatMessage> MessagesFor(string channelId)
        {
            return Cache.TryGetValue(channelId, out var messages)
                ? new List<CYGChatMessage>(messages)
                : new List<CYGChatMessage>();
        }

        /// <summary>Drops one channel's cache, or every channel when the id is empty.</summary>
        public static void Clear(string channelId = null)
        {
            if (string.IsNullOrEmpty(channelId)) Cache.Clear();
            else Cache.Remove(channelId);
        }

        /// <summary>The dotted channel id of a room stream: `2..<room>`.</summary>
        public static string RoomChannelId(string roomName) => "2.." + roomName;

        /// <summary>The dotted channel id of a group stream: `3.<group_id>..`.</summary>
        public static string GroupChannelId(string groupId) => "3." + groupId.ToLowerInvariant() + "..";

        /// <summary>The canonical dotted channel id of a direct stream: `4.<min>.<max>.`.</summary>
        public static string DirectChannelId(string userIdOne, string userIdTwo)
        {
            string one = userIdOne.ToLowerInvariant();
            string two = userIdTwo.ToLowerInvariant();
            return string.CompareOrdinal(one, two) <= 0
                ? $"4.{one}.{two}."
                : $"4.{two}.{one}.";
        }

        /// <summary>The `text` member of a content JSON string, or the raw content otherwise.</summary>
        internal static string ExtractText(string content)
        {
            if (string.IsNullOrEmpty(content)) return string.Empty;

            string trimmed = content.Trim();
            if (trimmed.StartsWith("{") && trimmed.EndsWith("}"))
            {
                try
                {
                    var parsed = JsonUtility.FromJson<CYGChatTextContent>(trimmed);
                    if (parsed != null && parsed.text != null) return parsed.text;
                }
                catch (Exception)
                {
                    // Not a text object; fall through to the raw content.
                }
            }

            return content;
        }

        private static async Task<CYGChatMessage> WriteAsync(
            string path, string contentJson, string channelId, bool edit)
        {
            string body = "{\"content\":" + contentJson + "}";
            var response = edit
                ? await CYGHttp.PutAsync(path, body)
                : await CYGHttp.PostAsync(path, body);

            if (!response.IsSuccess)
            {
                LogFailure(edit ? "Edit message" : "Send message", response);
                return null;
            }

            var message = Parse<CYGChatMessage>(response.Text, "message");
            if (message == null) return null;

            Merge(channelId, message);
            MessageReceived?.Invoke(message);
            NotifyUpdated(channelId);
            return message;
        }

        private static T Parse<T>(string text, string what) where T : class
        {
            if (string.IsNullOrEmpty(text)) return null;

            try
            {
                return JsonUtility.FromJson<T>(text);
            }
            catch (Exception exception)
            {
                Debug.LogError($"[CYGChat] Could not parse the {what}: {exception.Message}");
                return null;
            }
        }

        private static void Merge(string channelId, CYGChatMessage message)
        {
            if (!Cache.TryGetValue(channelId, out var messages))
            {
                messages = new List<CYGChatMessage>();
                Cache[channelId] = messages;
            }

            int index = messages.FindIndex(existing => existing.message_id == message.message_id);
            if (index >= 0)
            {
                messages[index] = message;
                return;
            }

            messages.Add(message);
            if (messages.Count > CacheLimit) messages.RemoveRange(0, messages.Count - CacheLimit);
        }

        private static void RemoveCached(string channelId, string messageId)
        {
            if (Cache.TryGetValue(channelId, out var messages))
                messages.RemoveAll(message => message.message_id == messageId);
        }

        private static void NotifyUpdated(string channelId)
        {
            MessagesUpdated?.Invoke(channelId, MessagesFor(channelId));
        }

        private static void LogFailure(string what, CYGHttp.HttpResponse response)
        {
            Debug.LogError($"[CYGChat] {what} failed: {response.StatusCode} - {response.Text}");
        }
    }
}
