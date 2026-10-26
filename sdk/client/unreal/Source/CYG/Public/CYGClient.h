#pragma once

#include "CoreMinimal.h"
#include "Sockets.h"
#include "SocketSubsystem.h"
#include "CYGClient.generated.h"

USTRUCT(BlueprintType)
struct FCYGEntity
{
	GENERATED_BODY()

	UPROPERTY(BlueprintReadOnly)
	int64 Id = 0;

	UPROPERTY(BlueprintReadOnly)
	FVector Position = FVector::ZeroVector;

	UPROPERTY(BlueprintReadOnly)
	float Yaw = 0.0f;

	UPROPERTY(BlueprintReadOnly)
	float Pitch = 0.0f;

	UPROPERTY(BlueprintReadOnly)
	float Health = 100.0f;

	UPROPERTY(BlueprintReadOnly)
	float Score = 0.0f;

	UPROPERTY(BlueprintReadOnly)
	bool bIsBot = false;
};

DECLARE_DYNAMIC_MULTICAST_DELEGATE_ThreeParams(FOnCYGDelta, const TArray<FCYGEntity>&, Entities, int64, LocalId, float, DeltaTime);

UCLASS(BlueprintType, Category = "CYG")
class CYG_API UCYGClient : public UObject
{
	GENERATED_BODY()

public:
	UFUNCTION(BlueprintCallable, Category = "CYG")
	bool ConnectToServer(const FString& Host, int32 Port, bool bSpectator = false);

	UFUNCTION(BlueprintCallable, Category = "CYG")
	void SendPlayerInput(uint32 Flags, float AimYaw, float AimPitch, float MoveX, float MoveZ);

	UFUNCTION(BlueprintCallable, Category = "CYG")
	void Disconnect();

	UPROPERTY(BlueprintAssignable, Category = "CYG")
	FOnCYGDelta OnDelta;

	void Tick(float DeltaTime);

private:
	FSocket* Socket = nullptr;
	TSharedPtr<FInternetAddr> ServerAddr;
	bool bIsConnected = false;
	bool bIsSpectator = false;
};
