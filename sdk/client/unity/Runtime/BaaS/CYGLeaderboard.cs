using System;
using System.Collections.Generic;
using System.Threading.Tasks;
using UnityEngine;
using UnityEngine.Networking;

namespace CookYourGame
{
    [Serializable]
    public class CYGLeaderboardRecord
    {
        public string leaderboard_id;
        public string owner_id;
        public string username;
        public long score;
        public long subscore;
        public int num_score;
        public string metadata;
        public string create_time;
        public string update_time;
        public long rank;
        public long max_num_score;
    }

    [Serializable]
    public class CYGLeaderboardRecordList
    {
        public List<CYGLeaderboardRecord> records = new List<CYGLeaderboardRecord>();
        public List<CYGLeaderboardRecord> owner_records = new List<CYGLeaderboardRecord>();
        public string next_cursor;
        public string prev_cursor;
    }

    [Serializable]
    internal class SubmitScorePayload
    {
        public long score;
        public long subscore;
        public string metadata;
    }

    /// <summary>
    /// Competitive Leaderboards and High Scores for CookYourGame.
    /// </summary>
    public static class CYGLeaderboard
    {
        /// <summary>
        /// Submits a player's score to a leaderboard.
        /// </summary>
        public static async Task<bool> SubmitScoreAsync(string leaderboardId, long score, long subscore = 0, string metadataJson = "{}")
        {
            var payload = new SubmitScorePayload
            {
                score = score,
                subscore = subscore,
                metadata = metadataJson
            };

            string body = JsonUtility.ToJson(payload);
            string path = $"/v2/leaderboard/{UnityWebRequest.EscapeURL(leaderboardId)}";
            var response = await CYGHttp.PostAsync(path, body);

            if (!response.IsSuccess)
            {
                Debug.LogError($"[CYGLeaderboard] Submit score failed: {response.StatusCode} - {response.Text}");
                return false;
            }

            return true;
        }

        /// <summary>
        /// Retrieves the top records from a leaderboard.
        /// </summary>
        public static async Task<CYGLeaderboardRecordList> GetScoresAsync(string leaderboardId, int limit = 10, string cursor = null)
        {
            string path = $"/v2/leaderboard/{UnityWebRequest.EscapeURL(leaderboardId)}?limit={limit}";
            if (!string.IsNullOrEmpty(cursor))
            {
                path += $"&cursor={UnityWebRequest.EscapeURL(cursor)}";
            }

            var response = await CYGHttp.GetAsync(path);
            if (!response.IsSuccess)
            {
                Debug.LogError($"[CYGLeaderboard] Get scores failed: {response.StatusCode} - {response.Text}");
                return null;
            }

            return JsonUtility.FromJson<CYGLeaderboardRecordList>(response.Text);
        }
    }
}
