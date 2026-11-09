// BaaS REST — the half of the SDK a game needs when nothing is moving.
//
// `CygClient` is the game loop: join, input, deltas, voice. That is the traffic
// that has to be fast and small. Everything else a game needs — a wallet, an
// inventory, a leaderboard, a cloud save, a purchase — is a request and a
// reply, and those belong on plain HTTP where a dropped packet is a retry
// rather than a desync.
//
// Unity's `UnityWebRequest` is deliberately not used: it only runs on the main
// thread and cannot be awaited from a background job, so every call here is
// `HttpClient` and the caller decides which thread awaits it.
using System;
using System.Collections.Generic;
using System.Net.Http;
using System.Text;
using System.Text.Json;
using System.Threading.Tasks;

namespace CYG
{
    /// <summary>What a request failed with, so a caller can branch on it.</summary>
    public sealed class CygApiException : Exception
    {
        public int Status { get; }
        public string Body { get; }

        public CygApiException(int status, string body)
            : base($"CYG API {status}: {Truncate(body)}")
        {
            Status = status;
            Body = body;
        }

        /// <summary>A 404 that means "this id is not yours" more often than "missing".</summary>
        public bool IsNotFound => Status == 404;
        /// <summary>The token is missing, expired, or belongs to another game.</summary>
        public bool IsUnauthorized => Status == 401 || Status == 403;
        /// <summary>The server refused the request on purpose — a rule, a quota, a balance.</summary>
        public bool IsRefused => Status == 400 || Status == 412;

        private static string Truncate(string text) =>
            string.IsNullOrEmpty(text) ? "" : (text.Length <= 200 ? text : text.Substring(0, 200));
    }

    /// <summary>
    /// The REST half of the SDK. One instance per signed-in player.
    /// </summary>
    /// <remarks>
    /// Bodies are built with <see cref="JsonSerializer"/> rather than string
    /// interpolation. The previous auth helper interpolated its arguments, so an
    /// email containing a quote produced invalid JSON and the server answered a
    /// parse error instead of a login failure — a confusing message for a
    /// mistyped password.
    /// </remarks>
    public sealed class CygRest
    {
        private readonly HttpClient _http;
        private readonly string _origin;
        private string _userId;

        public string Token { get; set; }

        public CygRest(string origin, string token = null)
        {
            _origin = (origin ?? "").TrimEnd('/');
            Token = token;
            _http = new HttpClient { Timeout = TimeSpan.FromSeconds(15) };
        }

        // ------------------------------------------------------------- plumbing

        private async Task<JsonElement> Send(HttpMethod method, string path, object body)
        {
            using var request = new HttpRequestMessage(method, _origin + path);
            if (!string.IsNullOrEmpty(Token))
                request.Headers.TryAddWithoutValidation("Authorization", "Bearer " + Token);
            if (body != null)
            {
                var json = JsonSerializer.Serialize(body);
                request.Content = new StringContent(json, Encoding.UTF8, "application/json");
            }

            using var response = await _http.SendAsync(request).ConfigureAwait(false);
            var text = await response.Content.ReadAsStringAsync().ConfigureAwait(false);
            if (!response.IsSuccessStatusCode)
                throw new CygApiException((int)response.StatusCode, text);

            if (string.IsNullOrWhiteSpace(text))
                return default;
            try
            {
                using var document = JsonDocument.Parse(text);
                // Cloned because the document is disposed with the response.
                return document.RootElement.Clone();
            }
            catch (JsonException)
            {
                // An endpoint that answers plain text is not a failure; the
                // caller asked for a value and got one it can read raw.
                return default;
            }
        }

        private Task<JsonElement> Get(string path) => Send(HttpMethod.Get, path, null);
        private Task<JsonElement> Post(string path, object body) => Send(HttpMethod.Post, path, body ?? new { });
        private Task<JsonElement> Put(string path, object body) => Send(HttpMethod.Put, path, body ?? new { });
        private Task<JsonElement> Delete(string path) => Send(HttpMethod.Delete, path, null);

        private static long AsLong(JsonElement element, string name)
        {
            if (element.ValueKind != JsonValueKind.Object) return 0;
            if (!element.TryGetProperty(name, out var value)) return 0;
            if (value.ValueKind == JsonValueKind.Number && value.TryGetInt64(out var number)) return number;
            // The wallet stores currency as a JSON number, but a value written by
            // the console can arrive as a string.
            if (value.ValueKind == JsonValueKind.String &&
                long.TryParse(value.GetString(), out var parsed)) return parsed;
            return 0;
        }

        // --------------------------------------------------------------- wallet

        /// <summary>The player's currencies, keyed by name.</summary>
        public async Task<Dictionary<string, double>> GetWalletAsync()
        {
            var root = await Get("/v2/account/wallet").ConfigureAwait(false);
            var wallet = new Dictionary<string, double>();
            if (root.ValueKind != JsonValueKind.Object) return wallet;
            if (!root.TryGetProperty("wallet", out var entries) ||
                entries.ValueKind != JsonValueKind.Object) return wallet;
            foreach (var property in entries.EnumerateObject())
            {
                if (property.Value.ValueKind == JsonValueKind.Number)
                    wallet[property.Name] = property.Value.GetDouble();
            }
            return wallet;
        }

        /// <summary>A single currency's balance, 0 when the player has never held it.</summary>
        public async Task<double> GetBalanceAsync(string currency)
        {
            var wallet = await GetWalletAsync().ConfigureAwait(false);
            return wallet.TryGetValue(currency, out var amount) ? amount : 0;
        }

        // ------------------------------------------------------------ inventory

        public async Task<IReadOnlyList<CygItem>> GetInventoryAsync()
        {
            var root = await Get("/v2/inventory").ConfigureAwait(false);
            var items = new List<CygItem>();
            if (root.ValueKind != JsonValueKind.Object) return items;
            if (!root.TryGetProperty("items", out var entries) ||
                entries.ValueKind != JsonValueKind.Array) return items;
            foreach (var entry in entries.EnumerateArray())
            {
                items.Add(new CygItem(
                    entry.TryGetProperty("itemId", out var id) ? id.GetString() : null,
                    (int)AsLong(entry, "quantity"),
                    entry.TryGetProperty("instanceId", out var instance) ? instance.GetString() : null));
            }
            return items;
        }

        /// <summary>How many of an item the player holds.</summary>
        public async Task<int> GetItemCountAsync(string itemId)
        {
            var items = await GetInventoryAsync().ConfigureAwait(false);
            var total = 0;
            foreach (var item in items)
                if (item.ItemId == itemId) total += item.Quantity;
            return total;
        }

        /// <summary>Spend one item. Refused when the player does not hold it.</summary>
        public Task ConsumeItemAsync(string itemId, int quantity = 1) =>
            Post("/v2/inventory/consume", new { item_id = itemId, quantity });

        // ---------------------------------------------------------- leaderboard

        public async Task<IReadOnlyList<CygScore>> ListScoresAsync(string leaderboardId, int limit = 10)
        {
            var root = await Get($"/v2/leaderboard/{Uri.EscapeDataString(leaderboardId)}?limit={limit}")
                .ConfigureAwait(false);
            var scores = new List<CygScore>();
            if (root.ValueKind != JsonValueKind.Object) return scores;
            if (!root.TryGetProperty("records", out var entries) ||
                entries.ValueKind != JsonValueKind.Array) return scores;
            foreach (var entry in entries.EnumerateArray())
            {
                scores.Add(new CygScore(
                    AsLong(entry, "score"),
                    AsLong(entry, "subscore"),
                    AsLong(entry, "rank"),
                    entry.TryGetProperty("username", out var name) ? name.GetString() : null));
            }
            return scores;
        }

        /// <summary>
        /// Create or update a board. Idempotent, so a game can declare its
        /// seasons at boot without checking whether they already exist.
        /// </summary>
        /// <param name="sortOrder">-1 when a lower score wins (a race time).</param>
        public Task<JsonElement> DefineBoardAsync(
            string leaderboardId, string title = null, short sortOrder = 1, string resetSchedule = null) =>
            Post($"/v2/leaderboard/{Uri.EscapeDataString(leaderboardId)}/define", new
            {
                title = title ?? leaderboardId,
                sort_order = (int)sortOrder,
                reset_schedule = resetSchedule ?? "",
            });

        /// <summary>Post a score. A lower-than-best score may be ignored by the board.</summary>
        public Task SubmitScoreAsync(string leaderboardId, long score, long subscore = 0) =>
            Post($"/v2/leaderboard/{Uri.EscapeDataString(leaderboardId)}",
                new { score, subscore });

        /// <summary>This player's own row, or null when they have never scored.</summary>
        public async Task<CygScore?> GetMyScoreAsync(string leaderboardId)
        {
            var root = await Get($"/v2/leaderboard/{Uri.EscapeDataString(leaderboardId)}/owner/me")
                .ConfigureAwait(false);
            if (root.ValueKind != JsonValueKind.Object) return null;
            if (!root.TryGetProperty("records", out var entries) ||
                entries.ValueKind != JsonValueKind.Array || entries.GetArrayLength() == 0)
                return null;
            var entry = entries[0];
            return new CygScore(
                AsLong(entry, "score"),
                AsLong(entry, "subscore"),
                AsLong(entry, "rank"),
                entry.TryGetProperty("username", out var name) ? name.GetString() : null);
        }

        // --------------------------------------------------------------- quests

        public async Task<IReadOnlyList<CygQuest>> GetQuestsAsync()
        {
            var root = await Get("/v2/quest").ConfigureAwait(false);
            var quests = new List<CygQuest>();
            if (root.ValueKind != JsonValueKind.Object) return quests;
            if (!root.TryGetProperty("quests", out var entries) ||
                entries.ValueKind != JsonValueKind.Array) return quests;
            foreach (var entry in entries.EnumerateArray())
            {
                quests.Add(new CygQuest(
                    entry.TryGetProperty("id", out var id) ? id.GetString() : null,
                    (int)AsLong(entry, "progress"),
                    (int)AsLong(entry, "goal"),
                    entry.TryGetProperty("completed", out var done) && done.ValueKind == JsonValueKind.True,
                    entry.TryGetProperty("claimed", out var claimed) && claimed.ValueKind == JsonValueKind.True));
            }
            return quests;
        }

        public Task ClaimQuestAsync(string questId) =>
            Post("/v2/quest/claim", new { quest_id = questId });

        /// <summary>
        /// Raise a counter. Server-authoritative games should send these from the
        /// room instead; a client that reports its own progress can inflate it.
        /// </summary>
        public Task TrackEventAsync(string name, int delta = 1) =>
            Post("/v2/event", new { events = new[] { new { name, value = delta } } });

        // -------------------------------------------------------- notifications

        public async Task<IReadOnlyList<CygNotification>> GetNotificationsAsync(int limit = 20)
        {
            var root = await Get($"/v2/notification?limit={limit}").ConfigureAwait(false);
            var notes = new List<CygNotification>();
            if (root.ValueKind != JsonValueKind.Object) return notes;
            if (!root.TryGetProperty("notifications", out var entries) ||
                entries.ValueKind != JsonValueKind.Array) return notes;
            foreach (var entry in entries.EnumerateArray())
            {
                notes.Add(new CygNotification(
                    entry.TryGetProperty("id", out var id) ? id.GetString() : null,
                    entry.TryGetProperty("subject", out var subject) ? subject.GetString() : null,
                    entry.TryGetProperty("content", out var content) ? content.GetRawText() : null,
                    entry.TryGetProperty("createTime", out var at) ? at.GetString() : null));
            }
            return notes;
        }

        public Task DeleteNotificationAsync(string id) =>
            Delete($"/v2/notification/{Uri.EscapeDataString(id)}");

        // -------------------------------------------------------------- storage

        /// <summary>
        /// The signed-in player's id, read from the token once.
        ///
        /// Storage addresses objects by owner, so the SDK needs it and the
        /// caller should not have to remember it.
        /// </summary>
        public async Task<string> GetUserIdAsync()
        {
            if (_userId != null) return _userId;
            // The account endpoint answers a flat object with `user_id`, not the
            // nested `{ user: { id } }` a REST-shaped API would use.
            var root = await Get("/v2/account").ConfigureAwait(false);
            if (root.ValueKind == JsonValueKind.Object &&
                root.TryGetProperty("user_id", out var id) &&
                id.ValueKind == JsonValueKind.String)
                _userId = id.GetString();
            return _userId;
        }

        /// <summary>Read a cloud-save object. Returns null when nothing is stored.</summary>
        public async Task<string> ReadStorageAsync(string key, string collection = "saves")
        {
            var userId = await GetUserIdAsync().ConfigureAwait(false);
            if (string.IsNullOrEmpty(userId))
                throw new InvalidOperationException("a storage read needs a signed-in player");
            try
            {
                var root = await Get(
                        $"/v2/storage/{Uri.EscapeDataString(collection)}/{Uri.EscapeDataString(key)}/{Uri.EscapeDataString(userId)}")
                    .ConfigureAwait(false);
                if (root.ValueKind != JsonValueKind.Object) return null;
                return root.TryGetProperty("value", out var value) ? value.GetRawText() : null;
            }
            catch (CygApiException error) when (error.IsNotFound)
            {
                // A save slot that does not exist yet is the first launch, not a
                // failure; every caller would otherwise write the same try/catch.
                return null;
            }
        }

        public Task WriteStorageAsync(string key, string json, string collection = "saves") =>
            Put("/v2/storage", new
            {
                collection,
                key,
                value = json,
                permission = "private",
            });

        public Task DeleteStorageAsync(string key, string collection = "saves") =>
            Put("/v2/storage/delete", new
            {
                objects = new[] { new { collection, key } },
            });

        // ------------------------------------------------------------ purchases

        /// <summary>
        /// Hand a store receipt to the server. The response says whether it was
        /// accepted; a rewarded purchase usually pays out through a rule or hook,
        /// so read the wallet afterwards rather than trusting a local guess.
        /// </summary>
        public Task<JsonElement> VerifyAppleAsync(string receipt, string productId = null) =>
            Post("/v2/iap/purchase/apple", new { receipt, product_id = productId });

        public Task<JsonElement> VerifyGoogleAsync(string receipt, string productId = null) =>
            Post("/v2/iap/purchase/google", new { receipt, product_id = productId });
    }

    public readonly struct CygItem
    {
        public readonly string ItemId;
        public readonly int Quantity;
        public readonly string InstanceId;

        public CygItem(string itemId, int quantity, string instanceId)
        {
            ItemId = itemId;
            Quantity = quantity;
            InstanceId = instanceId;
        }
    }

    public readonly struct CygScore
    {
        public readonly long Score;
        public readonly long Subscore;
        public readonly long Rank;
        public readonly string Username;

        public CygScore(long score, long subscore, long rank, string username)
        {
            Score = score;
            Subscore = subscore;
            Rank = rank;
            Username = username;
        }
    }

    public readonly struct CygQuest
    {
        public readonly string Id;
        public readonly int Progress;
        public readonly int Goal;
        public readonly bool Completed;
        public readonly bool Claimed;

        public CygQuest(string id, int progress, int goal, bool completed, bool claimed)
        {
            Id = id;
            Progress = progress;
            Goal = goal;
            Completed = completed;
            Claimed = claimed;
        }
    }

    public readonly struct CygNotification
    {
        public readonly string Id;
        public readonly string Subject;
        public readonly string Content;
        public readonly string CreateTime;

        public CygNotification(string id, string subject, string content, string createTime)
        {
            Id = id;
            Subject = subject;
            Content = content;
            CreateTime = createTime;
        }
    }
}
