using System;

namespace CookYourGame.Sim.Headless
{
    /// <summary>
    /// Predicting client. Local input is generated once per simulation tick;
    /// remote input arrives as redundant batches, is applied immediately, and
    /// past ticks are corrected through the session's ECS rollback.
    /// </summary>
    internal sealed class CYGMatchClient
    {
        private readonly int _totalTicks;
        private readonly int _redundancy;
        private readonly int _catchUpThreshold;
        private readonly int _peerPlayerId;
        private readonly CYGPlayerInput[] _history = new CYGPlayerInput[CYGInputRedundancy.MaxInputs];
        private readonly CYGPlayerInput[] _window = new CYGPlayerInput[CYGInputRedundancy.MaxInputs];
        private readonly byte[] _sendBuffer = new byte[512];
        private readonly uint[] _reportChecksums;

        private int _historyHead;
        private int _historyCount;
        private int _lastGeneratedTick = -1;
        private int _lastServerFrame = -1;
        private int _stalledStatuses;
        private int _nextMissingPeerTick;
        private bool _resendRequested;
        private bool _peerInputsComplete;

        public CYGMatchClient(
            int playerId,
            CYGWorldRollbackSession session,
            int totalTicks,
            int redundancy,
            int catchUpThreshold)
        {
            if (playerId <= 0)
            {
                throw new ArgumentOutOfRangeException(nameof(playerId));
            }
            if (totalTicks < 1)
            {
                throw new ArgumentOutOfRangeException(nameof(totalTicks));
            }
            if (redundancy < 1 || redundancy > CYGInputRedundancy.MaxInputs)
            {
                throw new ArgumentOutOfRangeException(nameof(redundancy));
            }

            PlayerId = playerId;
            Session = session;
            _totalTicks = totalTicks;
            _redundancy = redundancy;
            _catchUpThreshold = Math.Max(1, catchUpThreshold);
            _peerPlayerId = 3 - playerId;
            _reportChecksums = new uint[totalTicks];
        }

        public int PlayerId { get; }

        public CYGWorldRollbackSession Session { get; }

        public int GeneratedInputs { get; private set; }

        public int SentBatches { get; private set; }

        public int AppliedRelayBatches { get; private set; }

        public int CatchUpBatches { get; private set; }

        public int ReportsSeen { get; private set; }

        public bool ReportVerified { get; private set; }

        public int ReportMismatches { get; private set; }

        public int FirstMismatchFrame { get; private set; } = -1;

        public int LastServerFrame
        {
            get { return _lastServerFrame; }
        }

        public bool PeerInputsComplete
        {
            get { return _peerInputsComplete; }
        }

        /// <summary>
        /// Generates this tick's local input, sends a redundant window, and
        /// advances the prediction. Once the match is complete the client keeps
        /// re-sending its tail window so late losses can still be repaired.
        /// </summary>
        public void Advance(CYGNetworkSimulator network)
        {
            bool resendAll = _resendRequested || _stalledStatuses >= _catchUpThreshold;
            _resendRequested = false;
            int window = resendAll ? _history.Length : _redundancy;

            int tick = Session.CurrentTick;
            if (tick < _totalTicks)
            {
                if (tick > _lastGeneratedTick)
                {
                    CYGPlayerInput input = CYGBotController.Sample(PlayerId, tick);
                    Session.SetLocalInput(tick, input);
                    AddHistory(input);
                    _lastGeneratedTick = tick;
                }

                SendWindow(network, window);
                Session.Step();
                return;
            }

            SendWindow(network, window);
        }

        public void Receive(CYGNetPacket packet)
        {
            if (packet == null || packet.Payload == null || packet.Length < 1)
            {
                return;
            }

            CYGMatchMessage kind = (CYGMatchMessage)packet.Payload[0];
            switch (kind)
            {
                case CYGMatchMessage.RelayInput:
                    Session.ApplyRedundantInputPacket(
                        packet.Payload, CYGMatchProtocol.HeaderSize, packet.Length - CYGMatchProtocol.HeaderSize);
                    AppliedRelayBatches++;
                    break;

                case CYGMatchMessage.ServerStatus:
                    int serverFrame;
                    int missingPlayerId;
                    int missingTick;
                    if (CYGMatchProtocol.TryReadStatus(
                            packet.Payload, 0, packet.Length,
                            out serverFrame, out missingPlayerId, out missingTick))
                    {
                        OnServerStatus(serverFrame, missingPlayerId, missingTick);
                    }
                    break;

                case CYGMatchMessage.FinalReport:
                    TryApplyReport(packet.Payload, 0, packet.Length);
                    break;
            }
        }

        private void OnServerStatus(int serverFrame, int missingPlayerId, int missingTick)
        {
            if (serverFrame > _lastServerFrame)
            {
                _lastServerFrame = serverFrame;
                _stalledStatuses = 0;
            }
            else
            {
                _stalledStatuses++;
            }

            if (missingPlayerId == PlayerId && missingTick >= 0)
            {
                // The regular redundancy window already covers recent ticks;
                // only widen the window when the server asks for an older gap.
                int oldestCovered = Math.Max(0, _lastGeneratedTick - _redundancy + 1);
                if (missingTick < oldestCovered)
                {
                    _resendRequested = true;
                }
            }
        }

        private void TryApplyReport(byte[] payload, int offset, int length)
        {
            if (ReportVerified)
            {
                return;
            }
            if (Session.Frame < _totalTicks)
            {
                return;
            }
            if (!AllPeerInputsConfirmed())
            {
                return;
            }

            int count;
            if (!CYGMatchProtocol.TryReadReport(
                    payload, offset, length, _reportChecksums, out count))
            {
                return;
            }
            if (count != _totalTicks)
            {
                return;
            }

            ReportsSeen++;
            int mismatches = 0;
            int firstFrame = -1;
            for (int i = 0; i < count; i++)
            {
                int frame = i + 1;
                if (!Session.ConfirmFrame(frame, _reportChecksums[i]))
                {
                    mismatches++;
                    if (firstFrame < 0)
                    {
                        firstFrame = frame;
                    }
                }
            }

            ReportMismatches = mismatches;
            FirstMismatchFrame = firstFrame;
            ReportVerified = true;
        }

        private bool AllPeerInputsConfirmed()
        {
            if (_peerInputsComplete)
            {
                return true;
            }

            CYGInputRingBuffer ring = Session.InputRingBuffer;
            for (int tick = 0; tick < _totalTicks; tick++)
            {
                if (!ring.IsInputConfirmed(_peerPlayerId, tick))
                {
                    return false;
                }
            }

            _peerInputsComplete = true;
            return true;
        }

        private void AddHistory(in CYGPlayerInput input)
        {
            _history[_historyHead] = input;
            _historyHead = (_historyHead + 1) % _history.Length;
            if (_historyCount < _history.Length)
            {
                _historyCount++;
            }
            GeneratedInputs++;
        }

        private void SendWindow(CYGNetworkSimulator network, int requested)
        {
            int count = Math.Min(requested, _historyCount);
            if (count < 1)
            {
                return;
            }

            int start = (_historyHead - count + _history.Length) % _history.Length;
            for (int i = 0; i < count; i++)
            {
                _window[i] = _history[(start + i) % _history.Length];
            }

            int length = CYGMatchProtocol.WriteInput(
                _sendBuffer, PlayerId, _window, count, FindMissingPeerTick());
            network.Send(PlayerId, 0, _sendBuffer, 0, length);
            SentBatches++;
            if (requested > _redundancy)
            {
                CatchUpBatches++;
            }
        }

        /// <summary>
        /// Lowest peer tick not yet confirmed. Inputs only get added, so the
        /// scan is monotonic and stays cheap across the whole match.
        /// </summary>
        private int FindMissingPeerTick()
        {
            if (_peerInputsComplete)
            {
                return -1;
            }

            CYGInputRingBuffer ring = Session.InputRingBuffer;
            while (_nextMissingPeerTick < _totalTicks
                && ring.IsInputConfirmed(_peerPlayerId, _nextMissingPeerTick))
            {
                _nextMissingPeerTick++;
            }
            return _nextMissingPeerTick < _totalTicks ? _nextMissingPeerTick : -1;
        }
    }
}
