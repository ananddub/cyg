using CookYourGame.Sim;
using UnityEditor;
using UnityEngine;

namespace CookYourGame.EditorTools
{
    public static class CYGDeterminismMenu
    {
        [MenuItem("CookYourGame/Verify Physics Determinism")]
        public static void Verify()
        {
            var stopwatch = new System.Diagnostics.Stopwatch();
            stopwatch.Start();

            bool ok = CYGDeterminismProbe.Verify(out string detail);

            stopwatch.Stop();

            string message =
                $"[CYG] DETERMINISM {(ok ? "OK" : "FAIL")} (editor) {detail} " +
                $"elapsedMs={stopwatch.ElapsedMilliseconds}";

            if (ok)
            {
                Debug.Log(message);
            }
            else
            {
                Debug.LogError(message);
            }
        }
    }
}
