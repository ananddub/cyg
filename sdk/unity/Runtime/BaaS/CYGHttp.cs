using System;
using System.Text;
using System.Threading.Tasks;
using UnityEngine;
using UnityEngine.Networking;

namespace CookYourGame
{
    /// <summary>
    /// Internal HTTP transport for CookYourGame BaaS APIs (Auth, Inventory, Storage, Leaderboard).
    /// </summary>
    public static class CYGHttp
    {
        public static string BaseUrl = "http://127.0.0.1:8080";
        public static int TimeoutSeconds = 15;

        public static async Task<HttpResponse> SendAsync(string method, string path, string jsonBody = null, bool requireAuth = true)
        {
            string url = BaseUrl.TrimEnd('/') + (path.StartsWith("/") ? path : "/" + path);
            using (var request = new UnityWebRequest(url, method))
            {
                request.timeout = TimeoutSeconds;
                request.downloadHandler = new DownloadHandlerBuffer();

                if (!string.IsNullOrEmpty(jsonBody))
                {
                    byte[] raw = Encoding.UTF8.GetBytes(jsonBody);
                    request.uploadHandler = new UploadHandlerRaw(raw);
                    request.SetRequestHeader("Content-Type", "application/json");
                }

                if (requireAuth && !string.IsNullOrEmpty(CYGAuth.SessionToken))
                {
                    request.SetRequestHeader("Authorization", "Bearer " + CYGAuth.SessionToken);
                }

                var tcs = new TaskCompletionSource<HttpResponse>();
                var op = request.SendWebRequest();

                op.completed += _ =>
                {
                    bool isSuccess = request.result == UnityWebRequest.Result.Success;
                    long code = request.responseCode;
                    string text = request.downloadHandler?.text ?? string.Empty;
                    string error = request.error;

                    tcs.TrySetResult(new HttpResponse
                    {
                        IsSuccess = isSuccess,
                        StatusCode = code,
                        Text = text,
                        Error = error
                    });
                };

                return await tcs.Task;
            }
        }

        public static Task<HttpResponse> GetAsync(string path, bool requireAuth = true)
        {
            return SendAsync(UnityWebRequest.kHttpVerbGET, path, null, requireAuth);
        }

        public static Task<HttpResponse> PostAsync(string path, string jsonBody, bool requireAuth = true)
        {
            return SendAsync(UnityWebRequest.kHttpVerbPOST, path, jsonBody, requireAuth);
        }

        public static Task<HttpResponse> PutAsync(string path, string jsonBody, bool requireAuth = true)
        {
            return SendAsync(UnityWebRequest.kHttpVerbPUT, path, jsonBody, requireAuth);
        }

        public static Task<HttpResponse> DeleteAsync(string path, bool requireAuth = true)
        {
            return SendAsync(UnityWebRequest.kHttpVerbDELETE, path, null, requireAuth);
        }

        public struct HttpResponse
        {
            public bool IsSuccess;
            public long StatusCode;
            public string Text;
            public string Error;
        }
    }
}
