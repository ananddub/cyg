// Copyright CookYourGame. All rights reserved.

#pragma once

#include "CoreMinimal.h"
#include "Transport/CYGTransport.h"

class IWebSocket;

/**
 * WebSocket link built on the engine's own sockets, so the same module links on
 * Windows, Linux, Mac, iOS, Android and the web without an external library.
 *
 * Room traffic is binary: each socket message carries one wire message, and the
 * engine reassembles fragments before handing it over.
 */
class COOKYOURGAME_API FCYGWebSocketTransport : public FCYGTransport
{
public:
	FCYGWebSocketTransport() = default;
	~FCYGWebSocketTransport() override;

	void Open(const FString& Url, const FString& AuthToken) override;
	void SendPacket(const uint8* Packet, int32 Size) override;
	void Close() override;

private:
	void Attach(const TSharedRef<IWebSocket, ESPMode::ThreadSafe>& Socket);
	void HandleBinaryFrame(const void* Payload, int32 Size);
	void HandleClosed(uint32 ReasonCode, const FString& ReasonText);
	void HandleFailed(const FString& Reason);

	TSharedPtr<IWebSocket, ESPMode::ThreadSafe> Socket;
};
