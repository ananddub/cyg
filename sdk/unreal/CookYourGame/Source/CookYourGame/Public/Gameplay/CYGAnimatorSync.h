// Copyright CookYourGame. All rights reserved.

#pragma once

#include "CoreMinimal.h"
#include "Gameplay/CYGBehaviour.h"
#include "CYGAnimatorSync.generated.h"

class UAnimInstance;
class USkeletalMeshComponent;
class FProperty;

/** What a synced animation parameter holds. */
UENUM(BlueprintType)
enum class ECYGAnimatorParameterType : uint8
{
	Integer UMETA(DisplayName = "Integer"),
	Float		UMETA(DisplayName = "Float"),
	Bool		UMETA(DisplayName = "Bool"),
	Trigger		UMETA(DisplayName = "Trigger"),
};

/** One animation value that crosses the wire, named the way the anim graph names it. */
USTRUCT(BlueprintType)
struct FCYGAnimatorParameter
{
	GENERATED_BODY()

public:
	/** Name of the variable on the anim instance, or of the graph function a trigger calls. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "CookYourGame")
	FName Name;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "CookYourGame")
	ECYGAnimatorParameterType Type = ECYGAnimatorParameterType::Float;
};

/**
 * Networked animation state for a skeletal mesh.
 *
 * The authority polls the values it was told to carry and sends the ones that moved;
 * a trigger is not polled but fired, because it is an edge rather than a state. Both
 * sides address parameters by their position in the list, so the list has to be
 * authored the same way wherever the same actor exists.
 *
 * A trigger arrives as a call to the anim instance's function of that name, which is
 * how a Blueprint animation graph normally takes an event. A value write goes straight
 * to the variable, bypassing the anim instance's own interpolation.
 *
 * This blob is Unreal-specific: the Unity client drives an Animator, whose parameter
 * identity does not survive the trip. Spawn actors that use it on Unreal peers only.
 *
 * Blob after the base sync var block: [byte flags], bit0 values, bit1 triggers;
 * then [byte count]([byte index][value per type])* and [byte count]([byte index])*.
 */
UCLASS(ClassGroup = (CookYourGame), meta = (BlueprintSpawnableComponent, DisplayName = "CYG Animator Sync"))
class COOKYOURGAME_API UCYGAnimatorSync : public UCYGBehaviour
{
	GENERATED_BODY()

public:
	UCYGAnimatorSync();

	virtual void OnRegister() override;
	virtual void TickComponent(float DeltaTime, ELevelTick TickType, FActorComponentTickFunction* ThisTickFunction) override;

	/** Values this actor syncs, in wire order; sixty-four is the mask's width. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "CookYourGame|Network")
	TArray<FCYGAnimatorParameter> Parameters;

	/** Poll period for the values above; triggers are sent the moment they fire. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "CookYourGame|Network", meta = (ClampMin = "0.001"))
	float SampleIntervalSeconds = 0.02f;

	/** Smallest change worth a packet, so a settling blend does not talk continuously. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "CookYourGame|Network", meta = (ClampMin = "0.0"))
	float FloatDeadZone = 1e-4f;

	/** Mesh to drive; the owner's first skeletal mesh when left unset. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "CookYourGame|Network")
	TObjectPtr<USkeletalMeshComponent> Mesh;

	/** Fires the local graph function and, with authority, queues it for the room. */
	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	void FireTrigger(const FString& ParameterName);

	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	void SetFloatParameter(const FString& ParameterName, float Value);

	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	void SetBoolParameter(const FString& ParameterName, bool bValue);

	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	void SetIntegerParameter(const FString& ParameterName, int32 Value);

	virtual bool HasCYGDelta() const override;
	virtual void SerializeCYG(FCYGBitWriter& Sink) override;
	virtual void DeserializeCYG(FCYGBitReader& Source) override;

	virtual void OnSpawned() override;

protected:
	/** Parameter list plus the per-poll bookkeeping that goes with it. */
	void RebuildCarriedState();

private:
	static constexpr uint8 ValuesFlag = 1;
	static constexpr uint8 TriggersFlag = 2;

	/** Mask width, and the depth of the edge queue a burst of triggers can fill. */
	static constexpr int32 MaxCarriedParameters = 64;
	static constexpr int32 MaxQueuedTriggers = 8;

	int32 FindParameter(const FString& Name, ECYGAnimatorParameterType Type) const;

	/** Writes one declared parameter on the local anim instance. */
	void WriteParameter(const FString& Name, ECYGAnimatorParameterType Type, double Number);

	UAnimInstance* ResolveAnimInstance();
	void SampleAuthority();

	/** Reads and writes a value through the property, not the anim instance's setters, whose names moved between engine versions. */
	bool ReadValue(int32 Index, UAnimInstance* Instance, double& OutNumber) const;
	bool WriteValue(int32 Index, UAnimInstance* Instance, double Number) const;

	FProperty* FindProperty(int32 Index, const UAnimInstance* Instance) const;

	/** Calls the anim instance's function of this name, which is how a graph takes an event. */
	void InvokeTrigger(int32 Index, UAnimInstance* Instance) const;

	/** Parameters that are not triggers, in the order they were authored. */
	TArray<int32> Carried;

	/** One bit per carried slot, set when its value differs from the last send. */
	uint64 DirtyMask = 0;

	/** Last value seen for each carried slot, addressed by position in Carried. */
	TArray<double> LastNumbers;

	TArray<int32> PendingTriggers;

	TObjectPtr<UAnimInstance> AnimInstance = nullptr;
	double SampleUntilSeconds = 0.0;
	bool bHasBaseline = false;
};
