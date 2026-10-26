using System.IO;
using UnityEngine;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Drop this on a bootstrap object to prove that the current player build
    /// computes the same fixed-point golden vector as every other build.
    /// The result is logged with the platform and scripting backend so it can
    /// be collected from logcat, Xcode, or the WebGL console.
    /// </summary>
    [AddComponentMenu("CookYourGame/Sim/CYG Determinism Self Check")]
    public sealed class CYGDeterminismSelfCheck : MonoBehaviour
    {
        [SerializeField, Min(1)] private int iterations = CYGDeterminismProbe.DefaultIterations;
        [SerializeField] private int seed = unchecked((int)CYGDeterminismProbe.DefaultSeed);
        [SerializeField] private bool runOnStart = true;

        [Tooltip("Write the raw result stream next to persistentDataPath for byte-diffing two devices.")]
        [SerializeField] private bool dumpStream;

#if ENABLE_IL2CPP
        private const string Backend = "IL2CPP";
#elif ENABLE_MONO
        private const string Backend = "Mono";
#else
        private const string Backend = "Unknown";
#endif

        private void Start()
        {
            if (runOnStart)
            {
                RunCheck();
            }
        }

        [ContextMenu("Run Determinism Check")]
        public void RunCheck()
        {
            uint probeSeed = unchecked((uint)seed);
            var stopwatch = new System.Diagnostics.Stopwatch();
            stopwatch.Start();

            bool ok = CYGDeterminismProbe.Verify(iterations, probeSeed, out string detail);

            stopwatch.Stop();

            string message =
                $"[CYG] DETERMINISM {(ok ? "OK" : "FAIL")} platform={Application.platform} " +
                $"backend={Backend} unity={Application.unityVersion} {detail} " +
                $"elapsedMs={stopwatch.ElapsedMilliseconds}";

            if (ok)
            {
                Debug.Log(message);
            }
            else
            {
                Debug.LogError(message);
            }

            if (dumpStream)
            {
                byte[] stream = CYGDeterminismProbe.CreateStream(iterations, probeSeed, out _);
                string path = Path.Combine(Application.persistentDataPath, "cyg_determinism_stream.bin");
                File.WriteAllBytes(path, stream);
                Debug.Log($"[CYG] determinism stream ({stream.Length} bytes) written to {path}");
            }
        }
    }
}
