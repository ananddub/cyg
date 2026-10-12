#include "OpenOxideClient.h"
#include "Common/TcpSocketBuilder.h"
#include "Common/UdpSocketBuilder.h"

bool UOpenOxideClient::ConnectToServer(const FString& Host, int32 Port, bool bSpectator)
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

	Socket = FUdpSocketBuilder(TEXT("OpenOxideUdpSocket"))
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

void UOpenOxideClient::SendPlayerInput(uint32 Flags, float AimYaw, float AimPitch, float MoveX, float MoveZ)
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

void UOpenOxideClient::Tick(float DeltaTime)
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
			// Parse OpenOxide StateDelta
			if (BytesRead >= 12)
			{
				int32 Offset = 0;
				int64 LocalId = *(int64*)(RecvBuffer.GetData() + Offset);
				Offset += 8;

				uint32 PlayerCount = *(uint32*)(RecvBuffer.GetData() + Offset);
				Offset += 4;

				TArray<FOpenOxideEntity> Entities;
				for (uint32 i = 0; i < PlayerCount; i++)
				{
					if (Offset + 72 > BytesRead) break;

					FOpenOxideEntity Entity;
					Entity.Id = *(int64*)(RecvBuffer.GetData() + Offset); Offset += 8;
					double X = *(double*)(RecvBuffer.GetData() + Offset); Offset += 8;
					double Y = *(double*)(RecvBuffer.GetData() + Offset); Offset += 8;
					double Z = *(double*)(RecvBuffer.GetData() + Offset); Offset += 8;
					Entity.Position = FVector(X * 10.0f, Z * 10.0f, Y * 10.0f); // Coordinate mapping to Unreal cm
					Entity.Yaw = *(double*)(RecvBuffer.GetData() + Offset); Offset += 8;
					Entity.Pitch = *(double*)(RecvBuffer.GetData() + Offset); Offset += 8;
					Entity.Health = *(double*)(RecvBuffer.GetData() + Offset); Offset += 8;
					Entity.Score = *(double*)(RecvBuffer.GetData() + Offset); Offset += 8;
					Offset += 8; // state + pad

					Entity.bIsBot = (Entity.Id < 0);
					Entities.Add(Entity);
				}

				OnDelta.Broadcast(Entities, LocalId, DeltaTime);
			}
		}
	}
}

void UOpenOxideClient::Disconnect()
{
	if (Socket)
	{
		ISocketSubsystem::Get(PLATFORM_SOCKETSUBSYSTEM)->DestroySocket(Socket);
		Socket = nullptr;
	}
	bIsConnected = false;
}
