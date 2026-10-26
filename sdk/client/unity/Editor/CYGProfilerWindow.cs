using System;
using System.Text;
using UnityEngine;
using UnityEditor;

namespace CookYourGame.EditorTools
{
    /// <summary>
    /// Editor-only bar-chart companion to <see cref="CYGNetworkProfiler"/>. It reads the same
    /// static ring the runtime overlay uses and repaints at ~4 Hz, so it needs no transport
    /// wiring of its own. Open via Window/CookYourGame/Network Profiler. Lives in the
    /// CookYourGame.Editor assembly and is stripped from player builds entirely.
    /// </summary>
    public sealed class CYGProfilerWindow : EditorWindow
    {
        const int BarSamples = 40;
        const double RepaintIntervalSeconds = 0.25;

        readonly StringBuilder builder = new StringBuilder(512);
        Texture2D solidTexture;
        double lastRepaintAt = -999.0;
        bool showOpcodeTable;

        /// <summary>Opens the window from the menu.</summary>
        [MenuItem("Window/CookYourGame/Network Profiler")]
        public static void Open()
        {
            CYGProfilerWindow window = GetWindow<CYGProfilerWindow>("CYG Profiler");
            window.minSize = new Vector2(360f, 300f);
        }

        void OnEnable()
        {
            EditorApplication.update += Tick;
        }

        void OnDisable()
        {
            EditorApplication.update -= Tick;
        }

        void Tick()
        {
            double now = Time.realtimeSinceStartupAsDouble;
            if (now - lastRepaintAt < RepaintIntervalSeconds)
            {
                return;
            }

            lastRepaintAt = now;
            CYGNetworkProfiler.Update();
            Repaint();
        }

        void OnGUI()
        {
            Rect area = position;
            float left = area.x + 8f;
            float top = area.y + 6f;
            float width = Mathf.Max(160f, area.width - 16f);

            float enabledHeight = 22f;
            Rect toggle = new Rect(left, top, 130f, enabledHeight);
            if (GUI.Button(toggle, new GUIContent(CYGNetworkProfiler.Enabled ? "Recording: ON" : "Recording: OFF")))
            {
                CYGNetworkProfiler.Enabled = !CYGNetworkProfiler.Enabled;
            }

            Rect tableToggle = new Rect(left + 140f, top, 150f, enabledHeight);
            if (GUI.Button(tableToggle, new GUIContent(showOpcodeTable ? "Hide opcodes" : "Show opcodes")))
            {
                showOpcodeTable = !showOpcodeTable;
            }

            top += enabledHeight + 6f;
            Rect stats = new Rect(left, top, width, 76f);
            GUI.Label(stats, BuildSummary());

            float statsBottom = top + 76f;
            float tableHeight = showOpcodeTable ? 150f : 0f;
            DrawBars(statsBottom + 4f, width, Mathf.Max(60f, area.height - statsBottom - tableHeight - 16f));

            if (showOpcodeTable)
            {
                float tableTop = area.height - tableHeight - 6f;
                GUI.Label(new Rect(left, tableTop, width, tableHeight), BuildOpcodeTable());
            }
        }

        string BuildSummary()
        {
            CYGNetworkProfiler.Sample latest = CYGNetworkProfiler.LastSample;
            CYGNetworkProfiler.Snapshot snap = CYGNetworkProfiler.TakeSnapshot();

            double avgTickMs = latest.TickCount > 0 ? latest.TickSumSeconds / latest.TickCount * 1000.0 : 0.0;

            builder.Length = 0;
            builder.Append("out  ").Append(ToKilobytes(snap.LiveBytesSent)).AppendLine(" KB/s   in  ").Append(ToKilobytes(snap.LiveBytesReceived)).AppendLine(" KB/s");
            builder.Append("pkt  ").Append(latest.PacketsSent).AppendLine("/s out   ").Append(latest.PacketsReceived).AppendLine("/s in");
            builder.Append("tick ").Append(latest.TickCount).AppendLine(" Hz   avg ").Append(avgTickMs.ToString("0.0")).AppendLine(" ms   max ").Append(latest.MaxTickMs.ToString("0.0")).AppendLine(" ms");
            builder.Append("rtt  ").Append(CYGNetwork.Latency.ToString("0.0")).Append(" ms   room  ").Append(CYGNetwork.RoomName ?? "-");
            return builder.ToString();
        }

        string BuildOpcodeTable()
        {
            builder.Length = 0;
            builder.AppendLine("opcode            sent/s    recv/s");
            for (int op = 0; op < CYGNetworkProfiler.OpcodeCount; op++)
            {
                long sent;
                long received;
                CYGNetworkProfiler.GetOpcodeTotals(op, out sent, out received);
                if (sent == 0 && received == 0)
                {
                    continue;
                }

                builder.Append(((CYGOpCode)op).ToString().PadRight(16))
                       .Append(ToKilobytes(sent).PadLeft(7)).Append(" KB")
                       .Append(ToKilobytes(received).PadLeft(7)).Append(" KB")
                       .AppendLine();
            }

            AppendBreakdown("components", CYGNetworkProfiler.CommittedComponentTotals);
            AppendBreakdown("rpc", CYGNetworkProfiler.CommittedRpcTotals);
            return builder.ToString();
        }

        void AppendBreakdown(string header, System.Collections.Generic.IReadOnlyDictionary<string, long> totals)
        {
            if (totals == null || totals.Count == 0)
            {
                return;
            }

            builder.Append(header).AppendLine(":");
            foreach (System.Collections.Generic.KeyValuePair<string, long> entry in totals)
            {
                builder.Append("  ").Append(entry.Key).Append(' ').Append(entry.Value).AppendLine(" B");
            }
        }

        void DrawBars(float top, float width, float height)
        {
            CYGNetworkProfiler.Snapshot snap = CYGNetworkProfiler.TakeSnapshot();
            int count = snap.Count;
            if (count <= 0 || height <= 1f)
            {
                return;
            }

            int start = count > BarSamples ? count - BarSamples : 0;
            int bars = count - start;
            float barWidth = width / bars;
            long peak = 1;

            for (int i = start; i < count; i++)
            {
                CYGNetworkProfiler.Sample sample = snap.Samples[RingSlot(snap, i)];
                long total = sample.BytesSent + sample.BytesReceived;
                if (total > peak)
                {
                    peak = total;
                }
            }

            Texture2D tex = SolidTexture();
            for (int i = start; i < count; i++)
            {
                CYGNetworkProfiler.Sample sample = snap.Samples[RingSlot(snap, i)];
                float x = position.x + 8f + (i - start) * barWidth;
                float outHeight = height * Mathf.Clamp01((float)sample.BytesSent / peak);
                float inHeight = height * Mathf.Clamp01((float)sample.BytesReceived / peak);

                GUI.color = new Color(0.35f, 0.75f, 1f, 0.95f);
                GUI.DrawTexture(new Rect(x, top + (height - outHeight), barWidth * 0.45f, outHeight), tex);
                GUI.color = new Color(0.55f, 0.9f, 0.55f, 0.95f);
                GUI.DrawTexture(new Rect(x + barWidth * 0.5f, top + (height - inHeight), barWidth * 0.45f, inHeight), tex);
            }

            GUI.color = Color.white;
        }

        static int RingSlot(CYGNetworkProfiler.Snapshot snap, int ordinal)
        {
            return (snap.Head + ordinal) % snap.Capacity;
        }

        static string ToKilobytes(long bytes)
        {
            return (bytes / 1024f).ToString("0.0");
        }

        Texture2D SolidTexture()
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
}
