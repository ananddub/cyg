// Test-only UnityEngine shim: CygClient's Storage wrapper uses PlayerPrefs;
// a plain-dotnet run replaces it with an in-memory dictionary. Real Unity
// builds use the engine's own PlayerPrefs — this file never ships there.
using System.Collections.Generic;

namespace UnityEngine
{
    public static class PlayerPrefs
    {
        private static readonly Dictionary<string, string> Store = new Dictionary<string, string>();

        public static void SetString(string key, string value) => Store[key] = value;

        public static bool HasKey(string key) => Store.ContainsKey(key);

        public static string GetString(string key, string fallback = "") =>
            Store.TryGetValue(key, out var value) ? value : fallback;

        public static void DeleteKey(string key) => Store.Remove(key);
    }
}
