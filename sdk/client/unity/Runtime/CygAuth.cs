// BaaS auth — /v2/account/authenticate/* wrappers (web SDK client.auth parity).
using System;
using System.Net.Http;
using System.Text;
using System.Threading.Tasks;

namespace CYG
{
    public sealed class CygAuth
    {
        private readonly string _origin;
        public string Token { get; private set; }

        public CygAuth(string origin) { _origin = origin.TrimEnd('/'); }

        private async Task<string> Post(string path, string json)
        {
            using var http = new HttpClient { Timeout = TimeSpan.FromSeconds(8) };
            var res = await http.PostAsync($"{_origin}{path}",
                new StringContent(json, Encoding.UTF8, "application/json"));
            var body = await res.Content.ReadAsStringAsync();
            if (!res.IsSuccessStatusCode)
                throw new Exception($"auth failed: {res.StatusCode} {body}");
            Token = Extract(body, "token") ?? Token;
            return body;
        }

        public Task<string> LoginEmail(string email, string password) =>
            Post("/v2/account/authenticate/email",
                 $"{{\"email\":\"{email}\",\"password\":\"{password}\"}}");

        public Task<string> LoginDevice(string deviceId) =>
            Post("/v2/account/authenticate/device",
                 $"{{\"id\":\"{deviceId}\"}}");

        public Task<string> RegisterEmail(string email, string password, string username) =>
            Post("/v2/account/authenticate/email/register",
                 $"{{\"email\":\"{email}\",\"password\":\"{password}\",\"username\":\"{username}\"}}");

        private static string Extract(string json, string key)
        {
            int i = json.IndexOf($"\"{key}\"", StringComparison.Ordinal);
            if (i < 0) return null;
            int colon = json.IndexOf(':', i + key.Length + 2);
            int start = json.IndexOf('"', colon);
            if (start < 0) return null;
            int end = json.IndexOf('"', start + 1);
            return json.Substring(start + 1, end - start - 1);
        }
    }
}
