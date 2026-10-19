// Copyright CookYourGame. All rights reserved.

#pragma once

#include "CoreMinimal.h"
#include "Transport/CYGTransport.h"

/**
 * The same link, watched.
 *
 * Wraps a transport and passes every call straight through, so a room client cannot
 * tell the difference, and counts the bytes that go by in both directions into
 * FCYGNetworkProfiler. Installed by handing it to UCYGClient::UseTransport before the
 * first connection, or by the project setting that does the same at boot:
 *
 *   Client->UseTransport(MakeShared<FCYGProfilingTransport>(MakeShared<FCYGWebSocketTransport>()));
 *
 * The wrapper is the only owner of the transport it wraps, because its handlers reach
 * back into it: handing the inner link to somebody else would leave those handlers
 * pointing at freed memory the moment this wrapper is dropped.
 */
class COOKYOURGAME_API FCYGProfilingTransport : public FCYGTransport
{
public:
	explicit FCYGProfilingTransport(const TSharedPtr<FCYGTransport>& InInner);

	virtual void Open(const FString& Url, const FString& AuthToken) override;
	virtual void SendPacket(const uint8* Packet, int32 Size) override;
	virtual void Close() override;
	virtual void Tick(float DeltaSeconds) override;

private:
	/** Counts one inbound packet, then raises it unchanged. */
	void HandlePacket(const uint8* Packet, int32 Size);

	/** Reads the opcode and payload size off a canonical header. */
	static bool TryReadHeader(const uint8* Packet, int32 Size, ECYGOpCode& OutOp, int32& OutPayloadBytes);

	TSharedPtr<FCYGTransport> Inner;
};
