using System;
using System.Collections.Generic;
using System.Text;
using UnityEngine;

namespace CookYourGame
{
    /// <summary>
    /// Static developer-observability sink: rolls a 1-second window into a fixed ring of the
    /// last 60 samples, tracking bytes/packets per opcode, tick rate and per-component sync bytes.
    /// Every record call is a single branch when <see cref="Enabled"/> is off and allocates nothing
    /// on the hot path (fixed opcode arrays, no LINQ).
    /// </summary>
    public static class CYGNetworkProfiler
    {
        /// <summary>Number of one-second samples retained in the rolling history.</summary>
        public const int RingCapacity = 60;

        /// <summary>Highest opcode in <see cref="CYGOpCode"/> plus one; sizes the preallocated per-opcode arrays.</summary>
        public const int OpcodeCount = 22;

        const int MaxCommitCatchUp = 8;
        const int MaxTrackedComponents = 64;
        const int MaxTrackedRpcMethods = 64;

        /// <summary>One committed one-second sample of aggregate traffic.</summary>
        public struct Sample
        {
            /// <summary>Payload bytes sent during the window.</summary>
            public long BytesSent;

            /// <summary>Payload bytes received during the window.</summary>
            public long BytesReceived;

            /// <summary>Packets sent during the window.</summary>
            public int PacketsSent;

            /// <summary>Packets received during the window.</summary>
            public int PacketsReceived;

            /// <summary>Ticks recorded during the window (a 1-second window means this is the tick rate in Hz).</summary>
            public int TickCount;

            /// <summary>Summed tick durations in seconds, for averaging.</summary>
            public double TickSumSeconds;

            /// <summary>Longest single tick observed in the window, in milliseconds.</summary>
            public float MaxTickMs;

            /// <summary>Wall-clock second at which the window opened, from <see cref="Time.realtimeSinceStartupAsDouble"/>.</summary>
            public double StartedAt;
        }

        /// <summary>
        /// Stable, non-allocating view of the ring for UI consumers. The array fields alias the
        /// preallocated ring: read them, never mutate or retain them past the current frame.
        /// </summary>
        public struct Snapshot
        {
            /// <summary>The ring buffer itself (only <see cref="Count"/> entries are valid).</summary>
            public Sample[] Samples;

            /// <summary>Per-opcode sent bytes for each ring slot, indexed [slot, op].</summary>
            public long[,] OpcodeSent;

            /// <summary>Per-opcode received bytes for each ring slot, indexed [slot, op].</summary>
            public long[,] OpcodeReceived;

            /// <summary>Index of the oldest valid sample in the ring.</summary>
            public int Head;

            /// <summary>Number of valid samples (grows to <see cref="RingCapacity"/> then wraps).</summary>
            public int Count;

            /// <summary>Total slots in the ring (<see cref="RingCapacity"/>).</summary>
            public int Capacity;

            /// <summary>Second dimension of the per-opcode arrays (<see cref="OpcodeCount"/>).</summary>
            public int OpCount;

            /// <summary>In-progress (not yet committed) sent payload bytes for the current window.</summary>
            public long LiveBytesSent;

            /// <summary>In-progress (not yet committed) received payload bytes for the current window.</summary>
            public long LiveBytesReceived;

            /// <summary>In-progress sent packets for the current window.</summary>
            public int LivePacketsSent;

            /// <summary>In-progress received packets for the current window.</summary>
            public int LivePacketsReceived;
        }

        static readonly Sample[] ring = new Sample[RingCapacity];
        static readonly long[,] ringOpcodeSent = new long[RingCapacity, OpcodeCount];
        static readonly long[,] ringOpcodeReceived = new long[RingCapacity, OpcodeCount];

        static readonly long[] liveOpcodeSent = new long[OpcodeCount];
        static readonly long[] liveOpcodeReceived = new long[OpcodeCount];

        static readonly Dictionary<string, long> componentLive = new Dictionary<string, long>();
        static readonly Dictionary<string, long> componentCommitted = new Dictionary<string, long>();
        static readonly Dictionary<string, long> rpcLive = new Dictionary<string, long>();
        static readonly Dictionary<string, long> rpcCommitted = new Dictionary<string, long>();

        static long windowBytesSent;
        static long windowBytesReceived;
        static int windowPacketsSent;
        static int windowPacketsReceived;
        static int windowTicks;
        static double windowTickSumSeconds;
        static float windowMaxTickMs;

        static double windowOpenedAt;
        static bool windowStarted;
        static int writeIndex;
        static int fillCount;

        /// <summary>Master switch: when false every record call returns after a single branch and touches nothing.</summary>
        public static bool Enabled { get; set; }

        /// <summary>Payload bytes per second from the most recently committed window.</summary>
        public static long CurrentBytesPerSecondOut
        {
            get { return LatestCommitted().BytesSent; }
        }

        /// <summary>Payload bytes per second from the most recently committed window.</summary>
        public static long CurrentBytesPerSecondIn
        {
            get { return LatestCommitted().BytesReceived; }
        }

        /// <summary>Packets per second sent, from the most recently committed window.</summary>
        public static int PacketsPerSecondOut
        {
            get { return LatestCommitted().PacketsSent; }
        }

        /// <summary>Packets per second received, from the most recently committed window.</summary>
        public static int PacketsPerSecondIn
        {
            get { return LatestCommitted().PacketsReceived; }
        }

        /// <summary>The most recently committed one-second sample (default when none exist yet).</summary>
        public static Sample LastSample
        {
            get { return LatestCommitted(); }
        }

        /// <summary>
        /// Per-opcode payload byte totals for the most recently committed window.
        /// Falls back to the live in-progress window before the first sample commits.
        /// </summary>
        public static void GetOpcodeTotals(int op, out long sent, out long received)
        {
            if (op < 0 || op >= OpcodeCount)
            {
                sent = 0;
                received = 0;
                return;
            }

            if (fillCount > 0)
            {
                int slot = MostRecentSlot();
                sent = ringOpcodeSent[slot, op];
                received = ringOpcodeReceived[slot, op];
                return;
            }

            sent = liveOpcodeSent[op];
            received = liveOpcodeReceived[op];
        }

        /// <summary>Captures a non-allocating, frame-stable view of the ring and the live window.</summary>
        public static Snapshot TakeSnapshot()
        {
            Snapshot snapshot;
            snapshot.Samples = ring;
            snapshot.OpcodeSent = ringOpcodeSent;
            snapshot.OpcodeReceived = ringOpcodeReceived;
            snapshot.Head = fillCount == RingCapacity ? writeIndex : 0;
            snapshot.Count = fillCount;
            snapshot.Capacity = RingCapacity;
            snapshot.OpCount = OpcodeCount;
            snapshot.LiveBytesSent = windowBytesSent;
            snapshot.LiveBytesReceived = windowBytesReceived;
            snapshot.LivePacketsSent = windowPacketsSent;
            snapshot.LivePacketsReceived = windowPacketsReceived;
            return snapshot;
        }

        /// <summary>Committed component-type byte totals from the last window (read-only).</summary>
        public static IReadOnlyDictionary<string, long> CommittedComponentTotals
        {
            get { return componentCommitted; }
        }

        /// <summary>Committed rpc-method byte totals from the last window (read-only).</summary>
        public static IReadOnlyDictionary<string, long> CommittedRpcTotals
        {
            get { return rpcCommitted; }
        }

        /// <summary>Records outbound payload bytes (header stripped) for an opcode.</summary>
        public static void RecordSend(CYGOpCode op, int payloadBytes)
        {
            if (!Enabled)
            {
                return;
            }

            AdvanceWindow();
            int slot = OpcodeIndex(op);
            if (slot >= 0)
            {
                liveOpcodeSent[slot] += payloadBytes;
            }

            windowBytesSent += payloadBytes;
            windowPacketsSent++;
        }

        /// <summary>Records inbound payload bytes (header stripped) for an opcode.</summary>
        public static void RecordReceive(CYGOpCode op, int payloadBytes)
        {
            if (!Enabled)
            {
                return;
            }

            AdvanceWindow();
            int slot = OpcodeIndex(op);
            if (slot >= 0)
            {
                liveOpcodeReceived[slot] += payloadBytes;
            }

            windowBytesReceived += payloadBytes;
            windowPacketsReceived++;
        }

        /// <summary>Attributes payload bytes to a syncing component type for the current window.</summary>
        public static void RecordComponentSync(string componentTypeName, int payloadBytes)
        {
            if (!Enabled)
            {
                return;
            }

            if (string.IsNullOrEmpty(componentTypeName))
            {
                return;
            }

            AdvanceWindow();
            Accumulate(componentLive, componentTypeName, payloadBytes, MaxTrackedComponents);
        }

        /// <summary>Attributes payload bytes and one call to an rpc method for the current window.</summary>
        public static void RecordRpc(string methodName, int payloadBytes)
        {
            if (!Enabled)
            {
                return;
            }

            AdvanceWindow();
            string name = string.IsNullOrEmpty(methodName) ? "<unnamed>" : methodName;
            Accumulate(rpcLive, name, payloadBytes, MaxTrackedRpcMethods);
        }

        /// <summary>Adds one network tick duration (seconds) to the current window's tick statistics.</summary>
        public static void RecordTick(float tickSeconds)
        {
            if (!Enabled)
            {
                return;
            }

            AdvanceWindow();
            windowTicks++;
            windowTickSumSeconds += tickSeconds;
            float ms = tickSeconds * 1000f;
            if (ms > windowMaxTickMs)
            {
                windowMaxTickMs = ms;
            }
        }

        /// <summary>
        /// Advances the rolling window by wall-clock time, committing elapsed one-second windows
        /// into the ring. Idempotent within a frame; safe to call every update even while idle.
        /// </summary>
        public static void Update()
        {
            if (!Enabled)
            {
                return;
            }

            AdvanceWindow();
        }

        /// <summary>
        /// Force-commits the in-progress window immediately (used by tests and to snapshot a
        /// final partial second). No-op when nothing was accumulated.
        /// </summary>
        public static void CommitWindow()
        {
            if (!Enabled)
            {
                return;
            }

            Commit(Time.realtimeSinceStartupAsDouble);
        }

        /// <summary>Clears the ring, live window and breakdowns, leaving <see cref="Enabled"/> untouched.</summary>
        public static void Reset()
        {
            Array.Clear(ring, 0, ring.Length);
            Array.Clear(ringOpcodeSent, 0, ringOpcodeSent.Length);
            Array.Clear(ringOpcodeReceived, 0, ringOpcodeReceived.Length);
            Array.Clear(liveOpcodeSent, 0, liveOpcodeSent.Length);
            Array.Clear(liveOpcodeReceived, 0, liveOpcodeReceived.Length);
            componentLive.Clear();
            componentCommitted.Clear();
            rpcLive.Clear();
            rpcCommitted.Clear();
            ResetWindowAccumulators();
            writeIndex = 0;
            fillCount = 0;
            windowStarted = false;
        }

        static void AdvanceWindow()
        {
            double now = Time.realtimeSinceStartupAsDouble;
            if (!windowStarted)
            {
                windowOpenedAt = now;
                windowStarted = true;
                return;
            }

            int committed = 0;
            while (now - windowOpenedAt >= 1.0 && committed < MaxCommitCatchUp)
            {
                Commit(windowOpenedAt);
                windowOpenedAt += 1.0;
                committed++;
            }

            // A long stall collapses to a single fresh window rather than a burst of empty ones.
            if (now - windowOpenedAt >= 1.0)
            {
                windowOpenedAt = now;
            }
        }

        static void Commit(double startedAt)
        {
            Sample sample;
            sample.BytesSent = windowBytesSent;
            sample.BytesReceived = windowBytesReceived;
            sample.PacketsSent = windowPacketsSent;
            sample.PacketsReceived = windowPacketsReceived;
            sample.TickCount = windowTicks;
            sample.TickSumSeconds = windowTickSumSeconds;
            sample.MaxTickMs = windowMaxTickMs;
            sample.StartedAt = startedAt;

            ring[writeIndex] = sample;
            for (int op = 0; op < OpcodeCount; op++)
            {
                ringOpcodeSent[writeIndex, op] = liveOpcodeSent[op];
                ringOpcodeReceived[writeIndex, op] = liveOpcodeReceived[op];
            }

            SwapBreakdown(componentLive, componentCommitted);
            SwapBreakdown(rpcLive, rpcCommitted);

            writeIndex = writeIndex + 1 == RingCapacity ? 0 : writeIndex + 1;
            if (fillCount < RingCapacity)
            {
                fillCount++;
            }

            ResetWindowAccumulators();
        }

        static void ResetWindowAccumulators()
        {
            windowBytesSent = 0;
            windowBytesReceived = 0;
            windowPacketsSent = 0;
            windowPacketsReceived = 0;
            windowTicks = 0;
            windowTickSumSeconds = 0;
            windowMaxTickMs = 0f;
            Array.Clear(liveOpcodeSent, 0, liveOpcodeSent.Length);
            Array.Clear(liveOpcodeReceived, 0, liveOpcodeReceived.Length);
            componentLive.Clear();
            rpcLive.Clear();
        }

        static void SwapBreakdown(Dictionary<string, long> from, Dictionary<string, long> to)
        {
            to.Clear();
            foreach (KeyValuePair<string, long> entry in from)
            {
                to[entry.Key] = entry.Value;
            }

            from.Clear();
        }

        static void Accumulate(Dictionary<string, long> target, string key, int payloadBytes, int capacity)
        {
            long existing;
            if (target.TryGetValue(key, out existing))
            {
                target[key] = existing + payloadBytes;
                return;
            }

            if (target.Count >= capacity)
            {
                return;
            }

            target[key] = payloadBytes;
        }

        static int OpcodeIndex(CYGOpCode op)
        {
            int index = (int)op;
            return index >= 0 && index < OpcodeCount ? index : -1;
        }

        static int MostRecentSlot()
        {
            return writeIndex == 0 ? RingCapacity - 1 : writeIndex - 1;
        }

        static Sample LatestCommitted()
        {
            return fillCount > 0 ? ring[MostRecentSlot()] : default(Sample);
        }
    }

    /// <summary>
    /// On-screen profiling overlay. Compiled only for the editor and development builds so a
    /// shipping player strips the MonoBehaviour, its GUI code and the hotkey bootstrap entirely;
    /// the static <see cref="CYGNetworkProfiler"/> sink itself stays in every build so a custom
    /// UI or the transport decorator can still use it. Toggle visibility with Left Ctrl + P.
    /// </summary>
#if UNITY_EDITOR || DEVELOPMENT_BUILD
    [DisallowMultipleComponent]
    public sealed class CYGNetworkProfilerOverlay : MonoBehaviour
    {
        const float RedrawHz = 4f;
        const float PanelWidth = 320f;
        const float PanelHeight = 260f;
        const int BarSamples = 30;

        static CYGNetworkProfilerOverlay instance;

        readonly StringBuilder builder = new StringBuilder(256);
        Rect panel;
        bool visible;
        double lastRedrawAt = -999.0;
        string cached = string.Empty;

        /// <summary>Creates the persistent hidden overlay driver once per play session.</summary>
        [RuntimeInitializeOnLoadMethod(RuntimeInitializeLoadType.BeforeSceneLoad)]
        static void Boot()
        {
            if (instance != null)
            {
                return;
            }

            GameObject host = new GameObject("~CYGNetworkProfiler");
            instance = host.AddComponent<CYGNetworkProfilerOverlay>();
            DontDestroyOnLoad(host);
        }

        void Awake()
        {
            if (instance != null && instance != this)
            {
                Destroy(gameObject);
                return;
            }

            instance = this;
            panel = new Rect(12f, 12f, PanelWidth, PanelHeight);
        }

        void OnDestroy()
        {
            if (instance == this)
            {
                instance = null;
            }
        }

        static bool legacyInputAvailable = true;
        static bool newInputInitialized;
        static System.Reflection.PropertyInfo currentKeyboardProp;
        static System.Reflection.PropertyInfo pKeyProp;
        static System.Reflection.PropertyInfo leftCtrlKeyProp;
        static System.Reflection.PropertyInfo rightCtrlKeyProp;
        static System.Reflection.PropertyInfo wasPressedProp;
        static System.Reflection.PropertyInfo isPressedProp;

        static bool CheckToggleInput()
        {
            if (legacyInputAvailable)
            {
                try
                {
                    if (Input.GetKeyDown(KeyCode.P) && (Input.GetKey(KeyCode.LeftControl) || Input.GetKey(KeyCode.RightControl)))
                    {
                        return true;
                    }
                }
                catch (System.InvalidOperationException)
                {
                    // New Input System is active in PlayerSettings: disable legacy Input polling
                    legacyInputAvailable = false;
                }
            }

            if (!newInputInitialized)
            {
                newInputInitialized = true;
                try
                {
                    var kbType = System.Type.GetType("UnityEngine.InputSystem.Keyboard, Unity.InputSystem");
                    if (kbType != null)
                    {
                        currentKeyboardProp = kbType.GetProperty("current", System.Reflection.BindingFlags.Public | System.Reflection.BindingFlags.Static);
                        pKeyProp = kbType.GetProperty("pKey", System.Reflection.BindingFlags.Public | System.Reflection.BindingFlags.Instance);
                        leftCtrlKeyProp = kbType.GetProperty("leftCtrlKey", System.Reflection.BindingFlags.Public | System.Reflection.BindingFlags.Instance);
                        rightCtrlKeyProp = kbType.GetProperty("rightCtrlKey", System.Reflection.BindingFlags.Public | System.Reflection.BindingFlags.Instance);
                        if (pKeyProp != null)
                        {
                            var keyType = pKeyProp.PropertyType;
                            wasPressedProp = keyType.GetProperty("wasPressedThisFrame", System.Reflection.BindingFlags.Public | System.Reflection.BindingFlags.Instance);
                            isPressedProp = keyType.GetProperty("isPressed", System.Reflection.BindingFlags.Public | System.Reflection.BindingFlags.Instance);
                        }
                    }
                }
                catch { }
            }

            if (currentKeyboardProp != null && wasPressedProp != null)
            {
                try
                {
                    var kb = currentKeyboardProp.GetValue(null);
                    if (kb != null)
                    {
                        var pKey = pKeyProp.GetValue(kb);
                        if (pKey != null && (bool)wasPressedProp.GetValue(pKey))
                        {
                            var lctrl = leftCtrlKeyProp != null ? leftCtrlKeyProp.GetValue(kb) : null;
                            var rctrl = rightCtrlKeyProp != null ? rightCtrlKeyProp.GetValue(kb) : null;
                            bool lDown = lctrl != null && (bool)isPressedProp.GetValue(lctrl);
                            bool rDown = rctrl != null && (bool)isPressedProp.GetValue(rctrl);
                            return lDown || rDown;
                        }
                    }
                }
                catch { }
            }

            return false;
        }

        void Update()
        {
            if (CheckToggleInput())
            {
                visible = !visible;
                CYGNetworkProfiler.Enabled = visible || CYGNetworkProfiler.Enabled;
            }

            // Keep the rolling window honest even while idle so the history does not freeze.
            if (CYGNetworkProfiler.Enabled)
            {
                CYGNetworkProfiler.Update();
            }
        }

        void OnGUI()
        {
            Event e = Event.current;
            if (e != null && e.isKey && e.type == EventType.KeyDown && e.control && (e.keyCode == KeyCode.P || e.keyCode == (KeyCode)'p'))
            {
                visible = !visible;
                CYGNetworkProfiler.Enabled = visible || CYGNetworkProfiler.Enabled;
                e.Use();
            }

            if (!visible)
            {
                return;
            }

            double now = Time.realtimeSinceStartupAsDouble;
            if (now - lastRedrawAt >= 1.0 / RedrawHz)
            {
                lastRedrawAt = now;
                cached = BuildText();
            }

            GUI.Box(panel, GUIContent.none);
            Rect label = new Rect(panel.x + 8f, panel.y + 6f, panel.width - 16f, 150f);
            GUI.Label(label, cached);
            DrawBars();
        }

        string BuildText()
        {
            CYGNetworkProfiler.Snapshot snap = CYGNetworkProfiler.TakeSnapshot();
            CYGNetworkProfiler.Sample latest = CYGNetworkProfiler.LastSample;

            builder.Length = 0;
            builder.Append("CYG Network Profiler\n");
            builder.Append("out  ").Append((snap.LiveBytesSent / 1024f).ToString("0.0")).Append(" KB/s   in  ")
                   .Append((snap.LiveBytesReceived / 1024f).ToString("0.0")).AppendLine(" KB/s");
            builder.Append("pkt  ").Append(latest.PacketsSent)
                   .Append("/s out, ").Append(latest.PacketsReceived).Append("/s in").AppendLine();
            builder.Append("tick ").Append(latest.TickCount).Append(" Hz, avg ")
                   .Append((latest.TickCount > 0 ? (latest.TickSumSeconds / latest.TickCount * 1000.0) : 0.0).ToString("0.0"))
                   .Append(" ms, max ").Append(latest.MaxTickMs.ToString("0.0")).Append(" ms").AppendLine();
            builder.Append("rtt  ").Append(CYGNetwork.Latency.ToString("0.0")).Append(" ms")
                   .Append(CYGNetwork.IsConnected ? "  [connected]" : "  [offline]").AppendLine();
            AppendTopOpcodes();
            return builder.ToString();
        }

        void AppendTopOpcodes()
        {
            builder.Append("top:");
            int shown = 0;
            for (int op = 0; op < CYGNetworkProfiler.OpcodeCount && shown < 4; op++)
            {
                long sent;
                long received;
                CYGNetworkProfiler.GetOpcodeTotals(op, out sent, out received);
                long total = sent + received;
                if (total <= 0)
                {
                    continue;
                }

                builder.Append(' ').Append(((CYGOpCode)op).ToString()).Append(' ')
                       .Append(total.ToString()).Append('B');
                shown++;
            }
        }

        void DrawBars()
        {
            CYGNetworkProfiler.Snapshot snap = CYGNetworkProfiler.TakeSnapshot();
            int count = snap.Count;
            if (count <= 0)
            {
                return;
            }

            int start = count > BarSamples ? count - BarSamples : 0;
            int bars = count - start;
            float areaTop = panel.y + 160f;
            float areaHeight = panel.height - 172f;
            float barWidth = (panel.width - 16f) / bars;
            long peak = 1;

            for (int i = 0; i < bars; i++)
            {
                long total = snap.Samples[SampleIndex(snap, start + i)].BytesSent
                           + snap.Samples[SampleIndex(snap, start + i)].BytesReceived;
                if (total > peak)
                {
                    peak = total;
                }
            }

            for (int i = 0; i < bars; i++)
            {
                CYGNetworkProfiler.Sample sample = snap.Samples[SampleIndex(snap, start + i)];
                float x = panel.x + 8f + i * barWidth;
                float outHeight = areaHeight * Mathf.Clamp01((float)sample.BytesSent / peak);
                float inHeight = areaHeight * Mathf.Clamp01((float)sample.BytesReceived / peak);
                GUI.color = new Color(0.35f, 0.75f, 1f, 0.9f);
                GUI.DrawTexture(new Rect(x, areaTop + (areaHeight - outHeight), barWidth * 0.45f, outHeight), solid);
                GUI.color = new Color(0.55f, 0.9f, 0.55f, 0.9f);
                GUI.DrawTexture(new Rect(x + barWidth * 0.5f, areaTop + (areaHeight - inHeight), barWidth * 0.45f, inHeight), solid);
            }

            GUI.color = Color.white;
        }

        static int SampleIndex(CYGNetworkProfiler.Snapshot snap, int ordinal)
        {
            return (snap.Head + ordinal) % snap.Capacity;
        }

        static Texture2D solid
        {
            get
            {
                if (solidTexture == null)
                {
                    solidTexture = new Texture2D(1, 1);
                    solidTexture.SetPixel(0, 0, Color.white);
                    solidTexture.Apply();
                }

                return solidTexture;
            }
        }

        static Texture2D solidTexture;
    }
#endif
}
