using System;
using UnityEngine;
#if ENABLE_INPUT_SYSTEM
using UnityEngine.InputSystem;
#endif

namespace CookYourGame.Sim
{
    /// <summary>
    /// In-game desync and frame-difference overlay. It keeps a short ring of
    /// world snapshots, listens to <see cref="CYGVerifiedFrameTracker.OnDesync"/>
    /// and prints the first divergent component/entity from
    /// <see cref="CYGDesyncDiagnostics"/> directly on screen.
    ///
    /// Development aid only: snapshot capture allocates, so leave
    /// <see cref="captureSnapshots"/> disabled in shipping builds.
    /// </summary>
    [AddComponentMenu("CookYourGame/Sim/Desync Overlay")]
    public sealed class CYGDesyncOverlay : MonoBehaviour
    {
        [Tooltip("Runner to observe. Falls back to CYGDeterministicRunner.Instance or a scene search.")]
        [SerializeField] private CYGDeterministicRunner runner;

        [Tooltip("Capture a snapshot every simulated tick while the overlay is enabled.")]
        [SerializeField] private bool captureSnapshots = true;

        [Tooltip("How many recent frames to keep for the diff.")]
        [SerializeField] private int snapshotCapacity = 32;

        [Tooltip("Show the overlay as soon as a desync is detected.")]
        [SerializeField] private bool showOnDesync = true;

        [SerializeField] private KeyCode toggleKey = KeyCode.F8;
        [SerializeField] private KeyCode clearKey = KeyCode.F9;

        private CYGFrameSnapshotRing _snapshots;
        private CYGWorldRollbackSession _session;
        private CYGVerifiedFrameTracker _tracker;
        private CYGDesyncInfo _desync;
        private bool _hasDesync;
        private bool _visible;
        private string _diff = "No frame difference captured.";
        private int _lastCapturedTick = -1;
        private int _referenceTick = -1;
        private CYGWorldSnapshot _reference;

        public bool IsVisible => _visible;

        public CYGDesyncInfo Desync => _desync;

        public string LastDiff => _diff;

        private void Awake()
        {
            if (snapshotCapacity < 2)
            {
                snapshotCapacity = 2;
            }
            _snapshots = new CYGFrameSnapshotRing(snapshotCapacity);
        }

        private void OnEnable()
        {
            ResolveRunner();
            HookTracker(true);
        }

        private void OnDisable()
        {
            HookTracker(false);
        }

        private bool WasKeyPressed(KeyCode key)
        {
#if ENABLE_INPUT_SYSTEM
            var kb = Keyboard.current;
            if (kb != null)
            {
                switch (key)
                {
                    case KeyCode.F8: if (kb.f8Key.wasPressedThisFrame) return true; break;
                    case KeyCode.F9: if (kb.f9Key.wasPressedThisFrame) return true; break;
                    case KeyCode.F1: if (kb.f1Key.wasPressedThisFrame) return true; break;
                    case KeyCode.F2: if (kb.f2Key.wasPressedThisFrame) return true; break;
                    case KeyCode.F3: if (kb.f3Key.wasPressedThisFrame) return true; break;
                    case KeyCode.F4: if (kb.f4Key.wasPressedThisFrame) return true; break;
                    case KeyCode.F5: if (kb.f5Key.wasPressedThisFrame) return true; break;
                    case KeyCode.F6: if (kb.f6Key.wasPressedThisFrame) return true; break;
                    case KeyCode.F7: if (kb.f7Key.wasPressedThisFrame) return true; break;
                    case KeyCode.F10: if (kb.f10Key.wasPressedThisFrame) return true; break;
                    case KeyCode.F11: if (kb.f11Key.wasPressedThisFrame) return true; break;
                    case KeyCode.F12: if (kb.f12Key.wasPressedThisFrame) return true; break;
                    case KeyCode.Tab: if (kb.tabKey.wasPressedThisFrame) return true; break;
                    case KeyCode.BackQuote: if (kb.backquoteKey.wasPressedThisFrame) return true; break;
                }
            }
#endif
            try
            {
                return Input.GetKeyDown(key);
            }
            catch
            {
                return false;
            }
        }

        private void Update()
        {
            if (_session == null)
            {
                ResolveRunner();
                HookTracker(true);
            }

            if (WasKeyPressed(toggleKey))
            {
                _visible = !_visible;
            }
            if (WasKeyPressed(clearKey))
            {
                Clear();
            }

            if (captureSnapshots)
            {
                CaptureTickIfNeeded();
            }
        }

        private void OnGUI()
        {
            if (!_visible)
            {
                return;
            }

            GUILayout.BeginArea(new Rect(12f, 12f, Mathf.Min(560f, Screen.width - 24f), 340f), GUI.skin.box);
            GUILayout.BeginHorizontal();
            GUILayout.Label("<b>CYG Frame Differ</b>");
            GUILayout.FlexibleSpace();
            if (GUILayout.Button("Clear", GUILayout.Width(60f)))
            {
                Clear();
            }
            if (GUILayout.Button("Close", GUILayout.Width(60f)))
            {
                _visible = false;
            }
            GUILayout.EndHorizontal();

            CYGWorldRollbackSession session = _session;
            if (session == null)
            {
                GUILayout.Label("No CYGDeterministicRunner / rollback session found.");
                GUILayout.EndArea();
                return;
            }

            GUILayout.Label("Tick " + session.CurrentTick
                + "   confirmed " + session.ConfirmedTick
                + "   pending " + session.VerifiedFrames.PendingCount
                + "   checksum 0x" + session.Checksum.ToString("X8"));
            GUILayout.Label("Rollbacks " + session.TotalRollbacks
                + "   resim ticks " + session.TotalResimulatedTicks
                + "   rejected " + session.RejectedInputs);

            if (_hasDesync)
            {
                GUILayout.Label("<color=#FF5555><b>" + _desync + "</b></color>");
                GUILayout.Label("Mismatched frames " + session.VerifiedFrames.MismatchCount);
            }
            else
            {
                GUILayout.Label("<color=#55DD77>No desync recorded.</color>");
            }

            GUILayout.BeginHorizontal();
            if (GUILayout.Button("Diff stored vs current"))
            {
                RecomputeDiff();
            }
            if (GUILayout.Button("Capture reference"))
            {
                _reference = session.World.CreateSnapshot();
                _referenceTick = session.CurrentTick;
                _diff = "Reference captured at tick " + _referenceTick + ".";
            }
            GUILayout.EndHorizontal();

            GUILayout.Label(_diff, GUILayout.ExpandHeight(true));
            GUILayout.EndArea();
        }

        /// <summary>Supplies the authoritative snapshot so the overlay can diff local versus remote state.</summary>
        public void SetReferenceSnapshot(int tick, CYGWorldSnapshot snapshot)
        {
            if (snapshot == null)
            {
                return;
            }

            _reference = snapshot;
            _referenceTick = tick;
            RecomputeDiff();
        }

        /// <summary>Reports a desync from custom network code that already has both checksums.</summary>
        public void ReportDesync(int tick, uint localChecksum, uint remoteChecksum)
        {
            HandleDesync(new CYGDesyncInfo(tick, localChecksum, remoteChecksum));
        }

        private void HandleDesync(CYGDesyncInfo info)
        {
            _desync = info;
            _hasDesync = true;
            if (showOnDesync)
            {
                _visible = true;
            }
            RecomputeDiff();
        }

        private void RecomputeDiff()
        {
            if (_session == null || _session.World == null)
            {
                _diff = "No world to diff.";
                return;
            }

            if (_reference != null)
            {
                CYGWorldSnapshot current = _session.World.CreateSnapshot();
                _diff = CYGDesyncDiagnostics.Compare(
                    _referenceTick,
                    _reference,
                    _session.CurrentTick,
                    current);
                return;
            }

            CYGWorldSnapshot stored;
            int expectedTick = _hasDesync ? _desync.Tick : _session.CurrentTick;
            if (!_snapshots.TryGet(expectedTick, out stored))
            {
                _diff = _hasDesync
                    ? "No stored snapshot for tick " + _desync.Tick + ". Capture is disabled or the frame is outside the ring."
                    : "No stored frame yet. Enable snapshot capture and reproduce the desync.";
                return;
            }

            CYGWorldSnapshot actual = _session.World.CreateSnapshot();
            _diff = CYGDesyncDiagnostics.Compare(
                expectedTick,
                stored,
                _session.CurrentTick,
                actual);
        }

        private void CaptureTickIfNeeded()
        {
            if (_session == null || _session.World == null)
            {
                return;
            }

            int tick = _session.CurrentTick;
            if (tick == _lastCapturedTick)
            {
                return;
            }

            _lastCapturedTick = tick;
            _snapshots.Capture(tick, _session.World.CreateSnapshot());
        }

        private void ResolveRunner()
        {
            if (runner == null)
            {
                runner = CYGDeterministicRunner.Instance;
            }
            if (runner == null)
            {
#if UNITY_2023_1_OR_NEWER
                runner = FindFirstObjectByType<CYGDeterministicRunner>();
#else
                runner = FindObjectOfType<CYGDeterministicRunner>();
#endif
            }

            _session = runner != null ? runner.Session : null;
            _tracker = _session != null ? _session.VerifiedFrames : null;
        }

        private void HookTracker(bool subscribe)
        {
            if (_tracker == null)
            {
                return;
            }

            _tracker.OnDesync -= HandleDesync;
            if (subscribe)
            {
                _tracker.OnDesync += HandleDesync;
            }
        }

        private void Clear()
        {
            _snapshots.Clear();
            _lastCapturedTick = -1;
            _desync = default(CYGDesyncInfo);
            _hasDesync = false;
            _diff = "Cleared.";
        }
    }
}
