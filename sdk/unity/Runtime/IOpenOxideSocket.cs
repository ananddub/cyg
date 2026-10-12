using System;

namespace OpenOxide
{
    public interface IOpenOxideSocket : IDisposable
    {
        bool IsConnected { get; }
        event Action<byte[]> OnPacketReceived;
        event Action<Exception> OnError;
        void Send(byte[] data);
        void SendInput(PlayerInput input);
        void Disconnect();
    }
}
