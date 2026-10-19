using System;
using UnityEngine;

namespace CookYourGame
{
    /// <summary>
    /// Instrumenting decorator around a <see cref="CYGTransport"/>. It forwards the connect,
    /// send, close and pump calls to an inner transport, re-raises the base transport events so
    /// subscribers such as <c>CYGNetwork</c> keep working unchanged, and feeds the opcode and
    /// header-stripped payload size of every frame into <see cref="CYGNetworkProfiler"/>.
    /// Installed through a transport factory seam (no SDK edits) with a single line, e.g.:
    /// <c>CYGNetwork.TransportFactory = () =&gt; new CYGProfilingTransport(new CYGWebSocketTransport());</c>
    /// </summary>
    public sealed class CYGProfilingTransport : CYGTransport
    {
        readonly CYGTransport inner;

        /// <summary>Wraps a live transport instance; the wrapper subscribes to and owns its events.</summary>
        public CYGProfilingTransport(CYGTransport inner)
        {
            if (inner == null)
            {
                throw new ArgumentNullException("inner");
            }

            this.inner = inner;
            inner.OnConnected += HandleConnected;
            inner.OnDisconnected += HandleDisconnected;
            inner.OnFrame += HandleFrame;
            inner.OnError += HandleError;
        }

        /// <summary>The wrapped transport, exposed for teardown or inspection.</summary>
        public CYGTransport Inner
        {
            get { return inner; }
        }

        /// <summary>Opens the inner link; connected state propagates through the event handlers.</summary>
        public override void Connect(string url, string authToken)
        {
            inner.Connect(url, authToken);
        }

        /// <summary>Records the outbound opcode and payload size, then forwards the framed packet.</summary>
        public override void Send(ArraySegment<byte> framedPacket)
        {
            if (CYGNetworkProfiler.Enabled)
            {
                CYGOpCode op;
                int payloadBytes;
                if (TryDecode(framedPacket, out op, out payloadBytes))
                {
                    CYGNetworkProfiler.RecordSend(op, payloadBytes);
                }
            }

            inner.Send(framedPacket);
        }

        /// <summary>Tears the inner link down.</summary>
        public override void Close()
        {
            inner.Close();
        }

        /// <summary>
        /// Runs on the main thread from the base <see cref="CYGTransport.Pump"/>: mirrors the
        /// inner connection state onto this instance, then pumps the inner transport so its queued
        /// events fire our handlers, which in turn enqueue onto this instance's base event queue.
        /// </summary>
        protected override void OnMainThreadPump()
        {
            IsConnected = inner.IsConnected;
            inner.Pump();
        }

        void HandleConnected()
        {
            RaiseConnected();
        }

        void HandleDisconnected(string reason)
        {
            // The inner transport recorded the close frame's status during its own
            // drain, which has already run by the time this handler fires, so the
            // wrapper can mirror the code and keep the reject/leave distinction.
            RaiseDisconnected(reason, inner.LastCloseCode);
        }

        void HandleError(string detail)
        {
            RaiseError(detail);
        }

        void HandleFrame(ArraySegment<byte> frame)
        {
            if (CYGNetworkProfiler.Enabled)
            {
                CYGOpCode op;
                int payloadBytes;
                if (TryDecode(frame, out op, out payloadBytes))
                {
                    CYGNetworkProfiler.RecordReceive(op, payloadBytes);
                }
            }

            RaiseFrame(frame);
        }

        static bool TryDecode(ArraySegment<byte> framedPacket, out CYGOpCode op, out int payloadBytes)
        {
            op = default(CYGOpCode);
            payloadBytes = 0;

            if (framedPacket.Array == null || framedPacket.Count < CYGWireProtocol.HeaderSize)
            {
                return false;
            }

            byte rawOp = framedPacket.Array[framedPacket.Offset + 2];
            if (!CYGWireProtocol.IsCanonicalOp(rawOp))
            {
                return false;
            }

            op = (CYGOpCode)rawOp;
            payloadBytes = framedPacket.Count - CYGWireProtocol.HeaderSize;
            return true;
        }
    }
}
