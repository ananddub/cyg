// Copyright CookYourGame. All rights reserved.

#pragma once

#include "CoreMinimal.h"
#include "Transport/CYGWireProtocol.h"

DECLARE_MULTICAST_DELEGATE(FOnCYGTransportConnected);
DECLARE_MULTICAST_DELEGATE_TwoParams(FOnCYGTransportDisconnected, const FString& /*Reason*/, int32 /*CloseCode*/);
DECLARE_MULTICAST_DELEGATE_TwoParams(FOnCYGTransportPacket, const uint8* /*Packet*/, int32 /*Size*/);
DECLARE_MULTICAST_DELEGATE_OneParam(FOnCYGTransportError, const FString& /*Detail*/);

/**
 * Packet link to one room.
 *
 * The socket layer hands over wire messages; this base turns them into canonical
 * CookYourGame packets ([length u16 big-endian][opcode u8][payload]) and raises
 * them on the game thread. Handlers may read the packet pointer only for the
 * duration of the callback: it addresses the receive buffer, not a copy.
 */
class COOKYOURGAME_API FCYGTransport
{
public:
	virtual ~FCYGTransport() = default;

	/** Opens the link to a room endpoint, presenting an optional seat token. */
	virtual void Open(const FString& Url, const FString& AuthToken) = 0;

	/** Sends one complete canonical packet, header included. */
	virtual void SendPacket(const uint8* Packet, int32 Size) = 0;

	virtual void Close() = 0;

	/** Optional per-frame work for transports that queue sockets off-thread. */
	virtual void Tick(float DeltaSeconds) { (void)DeltaSeconds; }

	bool IsConnected() const { return bConnected; }
	int32 GetLastCloseCode() const { return LastCloseCode; }

	FOnCYGTransportConnected OnConnected;
	FOnCYGTransportDisconnected OnDisconnected;
	FOnCYGTransportPacket OnPacket;
	FOnCYGTransportError OnError;

protected:
	void NotifyConnected();
	void NotifyDisconnected(const FString& Reason, int32 CloseCode);
	void NotifyError(const FString& Detail);

	/** Translates one complete socket message into packet events. */
	void DeliverWireMessage(const uint8* Data, int32 Size);

	/** Wraps a payload in the canonical header and raises it. */
	void DeliverNormalizedPacket(ECYGOpCode Op, const uint8* Payload, int32 Size);

	/** Walks a run of back-to-back canonical packets, rejecting the run if any frame is malformed. */
	static bool CountCanonicalPackets(const uint8* Data, int32 Start, int32 Size, int32& OutPackets);

	bool bConnected = false;
	int32 LastCloseCode = 0;
};
