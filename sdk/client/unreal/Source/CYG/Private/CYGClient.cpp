#include "CYGClient.h"
#include "Common/TcpSocketBuilder.h"
#include "Common/UdpSocketBuilder.h"

bool UCYGClient::ConnectToServer(const FString& Host, int32 Port, bool bSpectator)
{
	Disconnect();
	bIsSpectator = bSpectator;

	ISocketSubsystem* SocketSubsystem = ISocketSubsystem::Get(PLATFORM_SOCKETSUBSYSTEM);
	if (!SocketSubsystem) return false;

	ServerAddr = SocketSubsystem->CreateInternetAddr();
	bool bIsValid = false;
	ServerAddr->SetIp(*Host, bIsValid);
	ServerAddr->SetPort(Port);

	if (!bIsValid) return false;

	Socket = FUdpSocketBuilder(TEXT("CYGUdpSocket"))
		.AsNonBlocking()
		.AsReusable()
		.WithReceiveBufferSize(65536)
		.WithSendBufferSize(65536)
		.Build();

	if (!Socket) return false;

	bIsConnected = true;

	if (bIsSpectator)
	{
		uint8 SpecHandshake[5] = { 0xFF, 0x53, 0x50, 0x45, 0x43 };
		int32 BytesSent = 0;
		Socket->SendTo(SpecHandshake, 5, BytesSent, *ServerAddr);
	}

	return true;
}

void UCYGClient::SendPlayerInput(uint32 Flags, float AimYaw, float AimPitch, float MoveX, float MoveZ)
{
	if (!bIsConnected || bIsSpectator || !Socket || !ServerAddr.IsValid()) return;

	uint8 Buffer[20];
	FMemory::Memcpy(Buffer, &Flags, 4);
	FMemory::Memcpy(Buffer + 4, &AimYaw, 4);
	FMemory::Memcpy(Buffer + 8, &AimPitch, 4);
	FMemory::Memcpy(Buffer + 12, &MoveX, 4);
	FMemory::Memcpy(Buffer + 16, &MoveZ, 4);

	int32 BytesSent = 0;
	Socket->SendTo(Buffer, 20, BytesSent, *ServerAddr);
}

void UCYGClient::Tick(float DeltaTime)
{
	if (!bIsConnected || !Socket) return;

	uint32 PendingSize = 0;
	while (Socket->HasPendingData(PendingSize))
	{
		TArray<uint8> RecvBuffer;
		RecvBuffer.SetNumUninitialized(PendingSize);

		int32 BytesRead = 0;
		TSharedRef<FInternetAddr> SenderAddr = ISocketSubsystem::Get(PLATFORM_SOCKETSUBSYSTEM)->CreateInternetAddr();
		if (Socket->RecvFrom(RecvBuffer.GetData(), RecvBuffer.Num(), BytesRead, *SenderAddr))
		{
			// The server frames every room delta as
			//   [0xA5]["delta"][i64 local_id][u32 entity_count][72B entities...]
			// followed by the ack/tick tail. Older CYG servers sent the
			// body without the 6-byte tag, so accept both layouts.
			int32 Offset = 0;
			if (BytesRead >= 6 && RecvBuffer[0] == 0xA5 && RecvBuffer[1] == static_cast<uint8>('d'))
			{
				Offset = 6;
			}

			if (BytesRead >= Offset + 12)
			{
				int64 LocalId = 0;
				FMemory::Memcpy(&LocalId, RecvBuffer.GetData() + Offset, sizeof(LocalId));
				Offset += 8;

				uint32 PlayerCount = 0;
				FMemory::Memcpy(&PlayerCount, RecvBuffer.GetData() + Offset, sizeof(PlayerCount));
				Offset += 4;

				// A corrupt datagram must not turn its declared count into a
				// multi-billion-iteration loop.
				const uint32 MaxEntities = 4096;
				if (PlayerCount > MaxEntities) PlayerCount = MaxEntities;

				TArray<FCYGEntity> Entities;
				for (uint32 i = 0; i < PlayerCount; i++)
				{
					if (Offset + 72 > BytesRead) break;

					FCYGEntity Entity;
					FMemory::Memcpy(&Entity.Id, RecvBuffer.GetData() + Offset, sizeof(Entity.Id)); Offset += 8;

					double X = 0.0, Y = 0.0, Z = 0.0;
					FMemory::Memcpy(&X, RecvBuffer.GetData() + Offset, sizeof(X)); Offset += 8;
					FMemory::Memcpy(&Y, RecvBuffer.GetData() + Offset, sizeof(Y)); Offset += 8;
					FMemory::Memcpy(&Z, RecvBuffer.GetData() + Offset, sizeof(Z)); Offset += 8;
					Entity.Position = FVector(X * 10.0f, Z * 10.0f, Y * 10.0f); // meters -> cm, Y-up -> Z-up

					double Yaw = 0.0, Pitch = 0.0, Health = 0.0, Score = 0.0;
					FMemory::Memcpy(&Yaw, RecvBuffer.GetData() + Offset, sizeof(Yaw)); Offset += 8;
					FMemory::Memcpy(&Pitch, RecvBuffer.GetData() + Offset, sizeof(Pitch)); Offset += 8;
					FMemory::Memcpy(&Health, RecvBuffer.GetData() + Offset, sizeof(Health)); Offset += 8;
					FMemory::Memcpy(&Score, RecvBuffer.GetData() + Offset, sizeof(Score)); Offset += 8;

					Entity.Yaw = static_cast<float>(Yaw);
					Entity.Pitch = static_cast<float>(Pitch);
					Entity.Health = static_cast<float>(Health);
					Entity.Score = static_cast<float>(Score);
					Offset += 8; // state + pad

					Entity.bIsBot = (Entity.Id < 0);
					Entities.Add(Entity);
				}

				OnDelta.Broadcast(Entities, LocalId, DeltaTime);
			}
		}
	}
}

void UCYGClient::Disconnect()
{
	if (Socket)
	{
		ISocketSubsystem::Get(PLATFORM_SOCKETSUBSYSTEM)->DestroySocket(Socket);
		Socket = nullptr;
	}
	bIsConnected = false;
}
