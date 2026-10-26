// Copyright CookYourGame. All rights reserved.

#pragma once

#include "CoreMinimal.h"
#include "Components/ActorComponent.h"
#include "CYGRpc.generated.h"

class UCYGView;
struct FCYGBitReader;
struct FCYGBitWriter;

/** Which clients execute an RPC; the byte value is what goes on the wire. */
UENUM(BlueprintType)
enum class ECYGRPCTarget : uint8
{
	/** Everyone, including the sender. */
	All			UMETA(DisplayName = "All Clients"),

	/** Everyone but the sender. */
	Others		UMETA(DisplayName = "Other Clients"),

	/** Spectators only. */
	Observers	UMETA(DisplayName = "Observers"),

	/** The client that leads the room. */
	MasterClient UMETA(DisplayName = "Master Client"),

	/** The client that owns the view. */
	Owner		UMETA(DisplayName = "View Owner"),

	/** One player, named when the call is made. */
	Target		UMETA(DisplayName = "One Player"),
};

DECLARE_DYNAMIC_MULTICAST_DELEGATE_TwoParams(FOnCYGUnboundRpc, uint16, MethodHash, const TArray<uint8>&, Arguments);

/**
 * Routes rpc frames between method names and UFunctions on the owning actor.
 *
 * Frame layout, matching the rest of the family of clients:
 *   [viewId u32 big-endian][method hash u16 big-endian][target u8][arguments].
 * The hash is FNV-1a over the ASCII method name, so a call site only needs the
 * name it wrote, never a registration id.
 */
struct COOKYOURGAME_API FCYGRpcRouter
{
	/** FNV-1a 16-bit hash of an ASCII method name. */
	static uint16 HashName(const FString& MethodName);

	/** Binds every method listed in AutoBindNames, plus metadata marked ones. */
	static void Bind(UObject* Owner, UCYGView* View, const TArray<FName>& MethodNames);

	static void Unbind(UCYGView* View);

	/** Drops every binding, called when the room link goes away. */
	static void Clear();

	/** Sends a call, then runs it here when the target includes this client. */
	static bool Send(UCYGView* View, const FString& MethodName, ECYGRPCTarget Target, const TArray<uint8>& Arguments, int32 TargetActorNumber = 0);

	/** Sends a call whose arguments a caller packed itself. */
	static bool SendPacked(UCYGView* View, const FString& MethodName, ECYGRPCTarget Target, FCYGBitWriter& Arguments, int32 TargetActorNumber = 0);

	/** Decodes one inbound frame and invokes the bound method. */
	static void HandleFrame(int32 ViewId, uint16 MethodHash, uint8 Target, const uint8* Arguments, int32 Size, const FOnCYGUnboundRpc* Unbound = nullptr);

	/** True when a locally initiated call should also execute here. */
	static bool ShouldEchoLocally(UCYGView* View, ECYGRPCTarget Target, int32 TargetActorNumber);

	/** Views holding at least one binding, for the profiler. */
	static int32 BoundViewCount();

	/** Raised when a frame names a method nothing bound. */
	static FOnCYGUnboundRpc OnUnboundRpc;
};

/**
 * Component face of the router: drop it on an actor, list the functions that
 * may be called remotely, and call the Rpc helpers from Blueprint or C++.
 */
UCLASS(ClassGroup = (CookYourGame), meta = (BlueprintSpawnableComponent, DisplayName = "CYG Rpc"))
class COOKYOURGAME_API UCYGRpcComponent : public UActorComponent
{
	GENERATED_BODY()

public:
	virtual void OnRegister() override;
	virtual void OnUnregister() override;

	/** Names of UFunctions on the owning actor that may receive rpc calls. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "CookYourGame|Network")
	TArray<FName> RpcMethodNames;

	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	void SendRpc(const FString& MethodName, ECYGRPCTarget Target, const TArray<uint8>& Arguments);

	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	void ServerRpc(const FString& MethodName, const TArray<uint8>& Arguments);

	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	void ObserversRpc(const FString& MethodName, const TArray<uint8>& Arguments);

	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	void TargetRpc(const FString& MethodName, int32 TargetActorNumber, const TArray<uint8>& Arguments);

	/** Raised when a call arrived for a method this actor does not implement. */
	UPROPERTY(BlueprintAssignable, Category = "CookYourGame|Network")
	FOnCYGUnboundRpc OnUnboundMethod;

protected:
	UPROPERTY()
	TObjectPtr<UCYGView> View;
};
