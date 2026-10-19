using System;
using System.Text;
using System.Threading.Tasks;
using UnityEngine;
using UnityEngine.Networking;

namespace CookYourGame
{
    [Serializable]
    public class CYGAuthSession
    {
        public bool created;
        public string token;
        public string refresh_token;
        public string userId;
        public string username;
    }

    [Serializable]
    internal class DeviceAuthBody
    {
        public string id;
    }

    [Serializable]
    internal class CustomAuthBody
    {
        public string id;
    }

    [Serializable]
    internal class EmailAuthBody
    {
        public string email;
        public string password;
    }

    [Serializable]
    internal class JwtClaimsPayload
    {
        public string uid;
        public string usn;
    }

    /// <summary>
    /// Authentication and player session management for CookYourGame.
    /// Provides 1-line async login (Device, Email, Custom ID) and token persistence.
    /// </summary>
    public static class CYGAuth
    {
        private const string PrefsTokenKey = "CYG_SESSION_TOKEN";
        private const string PrefsRefreshKey = "CYG_REFRESH_TOKEN";
        private const string PrefsUserIdKey = "CYG_USER_ID";
        private const string PrefsUsernameKey = "CYG_USERNAME";

        public static string SessionToken { get; private set; }
        public static string RefreshToken { get; private set; }
        public static string UserId { get; private set; }
        public static string Username { get; private set; }

        public static bool IsAuthenticated => !string.IsNullOrEmpty(SessionToken);

        public static event Action<CYGAuthSession> OnAuthenticated;
        public static event Action OnLoggedOut;

        static CYGAuth()
        {
            // Restore persisted session from PlayerPrefs if available
            SessionToken = PlayerPrefs.GetString(PrefsTokenKey, null);
            RefreshToken = PlayerPrefs.GetString(PrefsRefreshKey, null);
            UserId = PlayerPrefs.GetString(PrefsUserIdKey, null);
            Username = PlayerPrefs.GetString(PrefsUsernameKey, null);
        }

        /// <summary>
        /// Authenticates using the device unique identifier (guest one-click login).
        /// </summary>
        public static async Task<CYGAuthSession> LoginDeviceAsync(string deviceId = null, bool create = true, string username = null)
        {
            if (string.IsNullOrEmpty(deviceId))
            {
                deviceId = SystemInfo.deviceUniqueIdentifier;
                if (string.IsNullOrEmpty(deviceId) || deviceId == SystemInfo.unsupportedIdentifier)
                {
                    deviceId = Guid.NewGuid().ToString("N");
                }
            }

            string query = $"?create={create.ToString().ToLower()}";
            if (!string.IsNullOrEmpty(username))
            {
                query += $"&username={UnityWebRequest.EscapeURL(username)}";
            }

            string body = JsonUtility.ToJson(new DeviceAuthBody { id = deviceId });
            var response = await CYGHttp.PostAsync($"/v2/account/authenticate/device{query}", body, requireAuth: false);

            if (!response.IsSuccess)
            {
                Debug.LogError($"[CYGAuth] Device login failed: {response.StatusCode} - {response.Text}");
                return null;
            }

            return ProcessAuthResponse(response.Text);
        }

        /// <summary>
        /// Authenticates using email and password.
        /// </summary>
        public static async Task<CYGAuthSession> LoginEmailAsync(string email, string password, bool create = false, string username = null)
        {
            string query = $"?create={create.ToString().ToLower()}";
            if (!string.IsNullOrEmpty(username))
            {
                query += $"&username={UnityWebRequest.EscapeURL(username)}";
            }

            string body = JsonUtility.ToJson(new EmailAuthBody { email = email, password = password });
            var response = await CYGHttp.PostAsync($"/v2/account/authenticate/email{query}", body, requireAuth: false);

            if (!response.IsSuccess)
            {
                Debug.LogError($"[CYGAuth] Email login failed: {response.StatusCode} - {response.Text}");
                return null;
            }

            return ProcessAuthResponse(response.Text);
        }

        /// <summary>
        /// Authenticates using a custom user ID.
        /// </summary>
        public static async Task<CYGAuthSession> LoginCustomAsync(string customId, bool create = true, string username = null)
        {
            string query = $"?create={create.ToString().ToLower()}";
            if (!string.IsNullOrEmpty(username))
            {
                query += $"&username={UnityWebRequest.EscapeURL(username)}";
            }

            string body = JsonUtility.ToJson(new CustomAuthBody { id = customId });
            var response = await CYGHttp.PostAsync($"/v2/account/authenticate/custom{query}", body, requireAuth: false);

            if (!response.IsSuccess)
            {
                Debug.LogError($"[CYGAuth] Custom login failed: {response.StatusCode} - {response.Text}");
                return null;
            }

            return ProcessAuthResponse(response.Text);
        }

        /// <summary>
        /// Clears stored tokens and signs out locally.
        /// </summary>
        public static void Logout()
        {
            SessionToken = null;
            RefreshToken = null;
            UserId = null;
            Username = null;

            PlayerPrefs.DeleteKey(PrefsTokenKey);
            PlayerPrefs.DeleteKey(PrefsRefreshKey);
            PlayerPrefs.DeleteKey(PrefsUserIdKey);
            PlayerPrefs.DeleteKey(PrefsUsernameKey);
            PlayerPrefs.Save();

            OnLoggedOut?.Invoke();
        }

        private static CYGAuthSession ProcessAuthResponse(string jsonText)
        {
            var session = JsonUtility.FromJson<CYGAuthSession>(jsonText);
            if (session == null || string.IsNullOrEmpty(session.token))
            {
                return null;
            }

            SessionToken = session.token;
            RefreshToken = session.refresh_token;

            // Extract claims from JWT
            ParseJwtClaims(session.token, out string uid, out string usn);
            UserId = uid;
            Username = usn;
            session.userId = uid;
            session.username = usn;

            // Persist session
            PlayerPrefs.SetString(PrefsTokenKey, SessionToken);
            if (!string.IsNullOrEmpty(RefreshToken)) PlayerPrefs.SetString(PrefsRefreshKey, RefreshToken);
            if (!string.IsNullOrEmpty(UserId)) PlayerPrefs.SetString(PrefsUserIdKey, UserId);
            if (!string.IsNullOrEmpty(Username)) PlayerPrefs.SetString(PrefsUsernameKey, Username);
            PlayerPrefs.Save();

            OnAuthenticated?.Invoke(session);
            return session;
        }

        private static void ParseJwtClaims(string jwt, out string uid, out string usn)
        {
            uid = null;
            usn = null;
            try
            {
                string[] parts = jwt.Split('.');
                if (parts.Length < 2) return;

                string payload = parts[1];
                // Base64Url padding
                int pad = 4 - (payload.Length % 4);
                if (pad < 4) payload += new string('=', pad);
                payload = payload.Replace('-', '+').Replace('_', '/');

                byte[] bytes = Convert.FromBase64String(payload);
                string json = Encoding.UTF8.GetString(bytes);

                var claims = JsonUtility.FromJson<JwtClaimsPayload>(json);
                if (claims != null)
                {
                    uid = claims.uid;
                    usn = claims.usn;
                }
            }
            catch (Exception ex)
            {
                Debug.LogWarning($"[CYGAuth] Failed to decode JWT claims: {ex.Message}");
            }
        }
    }
}
