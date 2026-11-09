// BaaS auth — /v2/account/authenticate/* wrappers (web SDK client.auth parity).
//
// Bodies are serialised, never interpolated. The earlier version built its JSON
// with string interpolation, so an email containing a quote produced invalid
// JSON and the server answered a parse error instead of "wrong password" —
// which is the message the player needed.
using System;
using System.Net.Http;
using System.Text;
using System.Text.Json;
using System.Threading.Tasks;

namespace CYG
{
    public sealed class CygAuth
    {
        private readonly string _origin;
        private readonly HttpClient _http;
        private readonly string _serverKey;

        public string Token { get; private set; }

        /// <summary>
        /// <paramref name="serverKey"/> is the project's `control.session.server_key`.
        /// Account creation is server-to-server: a player session cannot make
        /// one, so a client that has never signed in has to present this. A game
        /// that only signs players in can leave it null.
        /// </summary>
        public CygAuth(string origin, string serverKey = null)
        {
            _origin = (origin ?? "").TrimEnd('/');
            _serverKey = serverKey;
            _http = new HttpClient { Timeout = TimeSpan.FromSeconds(15) };
        }

        private async Task<string> Post(string path, object body)
        {
            var json = JsonSerializer.Serialize(body);
            using var request = new HttpRequestMessage(HttpMethod.Post, $"{_origin}{path}")
            {
                Content = new StringContent(json, Encoding.UTF8, "application/json"),
            };
            if (!string.IsNullOrEmpty(_serverKey))
            {
                // The key travels as the Basic user, not as a Bearer token: the
                // middleware reads it from the user part of the header, so a
                // Bearer here would be refused as "Server key required".
                var basic = Convert.ToBase64String(Encoding.UTF8.GetBytes(_serverKey + ":"));
                request.Headers.TryAddWithoutValidation("Authorization", "Basic " + basic);
            }
            using var response = await _http.SendAsync(request).ConfigureAwait(false);
            var text = await response.Content.ReadAsStringAsync().ConfigureAwait(false);
            if (!response.IsSuccessStatusCode)
                throw new Exception($"auth failed: {(int)response.StatusCode} {text}");

            Token = Extract(text, "token") ?? Token;
            return text;
        }

        /// <summary>Sign in as a device. Creates the account when the id is new.</summary>
        /// <remark>
        /// The field is `id`, not `device_id` — the server reads `IdAccount`, and
        /// sending the wrong name authenticates nobody while still answering 200.
        /// </remark>
        public Task<string> LoginDevice(string deviceId) =>
            Post("/v2/account/authenticate/device", new { id = deviceId });

        public Task<string> LoginEmail(string email, string password) =>
            Post("/v2/account/authenticate/email", new { email, password });

        public Task<string> RegisterEmail(string email, string password, string username) =>
            Post("/v2/account/authenticate/email", new { email, password, username, create = true });

        private static string Extract(string json, string key)
        {
            int i = json.IndexOf($"\"{key}\"", StringComparison.Ordinal);
            if (i < 0) return null;
            int colon = json.IndexOf(':', i + key.Length + 2);
            int start = json.IndexOf('"', colon);
            if (start < 0) return null;
            int end = json.IndexOf('"', start + 1);
            return end < 0 ? null : json.Substring(start + 1, end - start - 1);
        }
    }
}
