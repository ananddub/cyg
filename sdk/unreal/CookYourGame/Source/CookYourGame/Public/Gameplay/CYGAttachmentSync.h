// Copyright CookYourGame. All rights reserved.

#pragma once

#include "CoreMinimal.h"
#include "Delegates/Delegate.h"
#include "Gameplay/CYGBehaviour.h"
#include "CYGAttachmentSync.generated.h"

class UCYGClient;
class USceneComponent;
struct FCYGPlayer;

/**
 * Keeps a mounted actor attached to the networked actor it rides on: a weapon in a
 * hand, a driver in a vehicle, a crate dropped on a moving platform.
 *
 * The mount rides its view's delta channel, so no new opcode is involved, and the blob
 * is the one the Unity client's hierarchy component reads and writes, field for field:
 *
 *   [byte flags]  bit0 hasParent, bit1 hasScale, bit2 hasSiblingIndex
 *   [uint varint parentViewId]  only when bit0 is set
 *   [packed relative location]  [relative rotation]
 *   [scale as three singles]    only when bit1 is set
 *   [byte siblingIndex]         only when bit2 is set
 *
 * The flags say what is present, so two peers with different settings on the component
 * still walk identical bytes. This component owns its whole blob and deliberately writes
 * no sync-var header, because a Unity mount sends none and a header byte would shift
 * every field after it: registering sync vars on a subclass here would break that.
 *
 * The parent travels as a view id, and the pose is measured against that view's actor
 * root, which is the basis a Unity peer measures from too. An offset authored against a
 * hand socket therefore goes out as a relative location rather than as a socket name a
 * peer may not be able to resolve. A child order number has no Unreal equivalent, so bit2
 * is consumed and ignored instead of produced.
 *
 * While the mount holds still the pose stays under the thresholds and nothing is sent.
 */
UCLASS(ClassGroup = (CookYourGame), meta = (BlueprintSpawnableComponent, DisplayName = "CYG Attachment Sync"))
class COOKYOURGAME_API UCYGAttachmentSync : public UCYGBehaviour
{
	GENERATED_BODY()

public:
	UCYGAttachmentSync();

	virtual void OnRegister() override;
	virtual void OnUnregister() override;

	virtual void TickComponent(float DeltaTime, ELevelTick TickType, FActorComponentTickFunction* ThisTickFunction) override;

	/** Carry the mount's scale too, which is authored rather than simulated. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "CookYourGame|Network")
	bool bSyncScale = true;

	/** How far the mount may shift, in centimetres, before its pose is worth sending. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "CookYourGame|Network", meta = (ClampMin = "0.0"))
	float PositionThresholdCentimetres = 0.01f;

	/** How far the mount may turn, in degrees, before its pose is worth sending. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "CookYourGame|Network", meta = (ClampMin = "0.0"))
	float RotationThresholdDegrees = 0.05f;

	UFUNCTION(BlueprintPure, Category = "CookYourGame|Network")
	USceneComponent* GetTargetComponent() const { return Target; }

	/** The networked actor this mount is riding, or 0 when it hangs off the world. */
	UFUNCTION(BlueprintPure, Category = "CookYourGame|Network")
	int32 GetMountedViewId() const;

	/** Forget the last sampled pose so the current mount is published again. */
	UFUNCTION(BlueprintCallable, Category = "CookYourGame|Network")
	void ResetAuthorityBaseline();

	virtual bool HasCYGDelta() const override;
	virtual void SerializeCYG(FCYGBitWriter& Sink) override;
	virtual void DeserializeCYG(FCYGBitReader& Source) override;

	virtual void OnSpawned() override;
	virtual void OnOwnerChanged(const FCYGPlayer& NewOwner) override;

	/** Wire flags for the mount fields a delta carries; see the class comment. */
	static constexpr uint8 ParentFlag = 1;
	static constexpr uint8 ScaleFlag = 2;

	/** Set by a Unity peer for its child order number; this side only skips the byte. */
	static constexpr uint8 SiblingFlag = 4;

protected:
	/** What travels; the owning actor's root unless this says otherwise. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "CookYourGame|Network")
	TObjectPtr<USceneComponent> Target;

private:
	/** Where a patch stands between "the parent is not here yet" and "never will be". */
	enum class EParentResolution : uint8
	{
		/** The id is resolved, or is zero for a mount that hangs off the world. */
		Ready,
		/** A non-zero id names a view this peer has not spawned yet. */
		Waiting,
		/** The view is here but has nothing to hang a mount on. */
		Unusable,
	};

	void ResolveTarget();
	void SampleAuthority();
	void Publish(int32 ParentViewId, const FVector& Location, const FQuat& Rotation, const FVector& Scale);
	void CommitBaseline(int32 ParentViewId, const FVector& Location, const FQuat& Rotation);

	/** The parent root a pose is measured against, plus the view id that names it. */
	USceneComponent* ResolveParentRoot(int32& OutViewId) const;

	EParentResolution ResolveParent(int32 ParentViewId, USceneComponent*& OutParent) const;

	/** Drops a patch whose parent is here but unusable, naming it once. */
	void RejectUnusableParent(int32 ParentViewId);

	void ApplyOrQueue(int32 ParentViewId, bool bHasScale, const FVector& Location, const FQuat& Rotation, const FVector& Scale);
	void QueuePending(int32 ParentViewId, bool bHasScale, const FVector& Location, const FQuat& Rotation, const FVector& Scale);
	void RetryPending();

	/** False when the mount was refused, which is final rather than worth retrying. */
	bool ApplyMount(USceneComponent* Parent, bool bHasScale, const FVector& Location, const FQuat& Rotation, const FVector& Scale);

	/** True when mounting on Parent would put this actor inside itself. */
	bool IsMountedInsideSelf(const USceneComponent* Parent) const;

	void ClearPending();

	/** Subscribes to despawns so a patch waiting on a parent stops waiting when it goes. */
	void BindClient();
	void UnbindClient();

	void HandleDespawn(int32 ViewId);

	/** Authority sampling state. */
	bool bDirty = false;
	bool bHasBaseline = false;
	bool bHadAuthority = false;
	int32 LastParentViewId = 0;
	FVector LastLocation = FVector::ZeroVector;
	FQuat LastRotation = FQuat::Identity;

	/** The snapshot the next pack writes. */
	bool bOutHasParent = false;
	bool bOutHasScale = false;
	int32 OutParentViewId = 0;
	FVector OutLocation = FVector::ZeroVector;
	FQuat OutRotation = FQuat::Identity;
	FVector OutScale = FVector::OneVector;

	/** A patch whose parent has not arrived, and the budget for waiting on it. */
	bool bHasPending = false;
	bool bPendingHasScale = false;
	int32 PendingParentViewId = 0;
	FVector PendingLocation = FVector::ZeroVector;
	FQuat PendingRotation = FQuat::Identity;
	FVector PendingScale = FVector::OneVector;
	int32 PendingAttempts = 0;

	bool bPendingGaveUpWarned = false;
	bool bWarnedStaticMobility = false;
	bool bWarnedUnusableParent = false;

	TWeakObjectPtr<UCYGClient> BoundClient;
	FDelegateHandle DespawnHandle;
};
