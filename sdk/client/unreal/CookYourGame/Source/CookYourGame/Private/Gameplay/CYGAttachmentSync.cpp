// Copyright CookYourGame. All rights reserved.

#include "Gameplay/CYGAttachmentSync.h"

#include "Components/SceneComponent.h"
#include "CookYourGame.h"
#include "Core/CYGClient.h"
#include "GameFramework/Actor.h"
#include "Gameplay/CYGCoordinateSpace.h"
#include "Gameplay/CYGPoseInterpolator.h"
#include "Gameplay/CYGView.h"
#include "Serialization/CYGBitPacker.h"

namespace
{
	/**
	 * How many ticks a proxy waits on a parent view it has not spawned. Bounded retries
	 * cover a spawn that is still in flight; past the budget the mount was never meant
	 * for this peer and the patch is dropped instead of queued forever.
	 */
	constexpr int32 MaxPendingMountAttempts = 60;

	/** A mount that never settles is a mount that sends every frame, so the floor holds. */
	constexpr double MinimumThresholdCentimetres = 0.01;
}

UCYGAttachmentSync::UCYGAttachmentSync()
{
	PrimaryComponentTick.bCanEverTick = true;

	// Post-update, so the pose is read after gameplay and after any transform sync have
	// written the component this frame: the mount the room sees is the one at end of frame.
	PrimaryComponentTick.TickGroup = TG_PostUpdateWorld;
}

void UCYGAttachmentSync::OnRegister()
{
	// Before Super, so the tick function is created already knowing it may run.
	PrimaryComponentTick.bCanEverTick = true;

	Super::OnRegister();

	ResolveTarget();
	RegisterAsObservable();
	BindClient();

	bHasBaseline = false;
	bHadAuthority = HasAuthority();
}

void UCYGAttachmentSync::OnUnregister()
{
	UnbindClient();
	ClearPending();

	Super::OnUnregister();
}

void UCYGAttachmentSync::ResolveTarget()
{
	if (Target == nullptr && GetOwner() != nullptr)
	{
		Target = GetOwner()->GetRootComponent();
	}
}

void UCYGAttachmentSync::OnSpawned()
{
	bHasBaseline = false;
	bDirty = false;
	ClearPending();
}

void UCYGAttachmentSync::OnOwnerChanged(const FCYGPlayer& NewOwner)
{
	(void)NewOwner;

	// Whoever holds the view now has to state the mount once, from its own transform.
	bHasBaseline = false;
	bDirty = false;
	ClearPending();
}

void UCYGAttachmentSync::TickComponent(float DeltaTime, ELevelTick TickType, FActorComponentTickFunction* ThisTickFunction)
{
	Super::TickComponent(DeltaTime, TickType, ThisTickFunction);

	if (Target == nullptr)
	{
		ResolveTarget();

		if (Target == nullptr)
		{
			return;
		}
	}

	const bool bAuthority = HasAuthority();

	if (bAuthority != bHadAuthority)
	{
		// Ownership flipped: drop the baseline so the new authority publishes the mount once,
		// and forget a queued patch it is no longer entitled to apply.
		bHadAuthority = bAuthority;
		bHasBaseline = false;
		bDirty = false;

		if (bAuthority)
		{
			ClearPending();
			bPendingGaveUpWarned = false;
		}
	}

	if (bAuthority)
	{
		SampleAuthority();

		return;
	}

	if (bHasPending)
	{
		// A level actor can register before the room client exists, so bind again here.
		BindClient();
		RetryPending();
	}
}

void UCYGAttachmentSync::SampleAuthority()
{
	int32 ParentViewId = 0;
	const USceneComponent* ParentRoot = ResolveParentRoot(ParentViewId);

	const FTransform ParentWorld = ParentRoot != nullptr
		? ParentRoot->GetComponentTransform()
		: FTransform::Identity;

	// Measured against the parent root rather than read off AttachParent, because the
	// attachment may be a socket halfway down the parent's component chain while every
	// peer resolves this mount against the root of the actor that id names.
	const FTransform Mount = Target->GetComponentTransform() * ParentWorld.Inverse();

	const FVector Location = Mount.GetTranslation();
	const FQuat Rotation = Mount.GetRotation();

	if (!bHasBaseline)
	{
		Publish(ParentViewId, Location, Rotation, Target->GetRelativeScale3D());
		CommitBaseline(ParentViewId, Location, Rotation);
		bDirty = true;

		return;
	}

	const double ThresholdSquared = FMath::Max(MinimumThresholdCentimetres, static_cast<double>(PositionThresholdCentimetres));
	const bool bParentChanged = ParentViewId != LastParentViewId;
	const bool bMoved = FVector::DistanceSquared(Location, LastLocation) >= ThresholdSquared
		|| CYGPoseMath::AngleBetween(LastRotation, Rotation) > FMath::DegreesToRadians(RotationThresholdDegrees);

	if (bParentChanged || bMoved)
	{
		Publish(ParentViewId, Location, Rotation, Target->GetRelativeScale3D());
		CommitBaseline(ParentViewId, Location, Rotation);
		bDirty = true;
	}
}

USceneComponent* UCYGAttachmentSync::ResolveParentRoot(int32& OutViewId) const
{
	OutViewId = 0;

	const AActor* Owner = GetOwner();
	USceneComponent* Ancestor = Target != nullptr ? Target->GetAttachParent() : nullptr;

	while (Ancestor != nullptr)
	{
		const AActor* AncestorOwner = Ancestor->GetOwner();

		// A component of this same actor is a step up the chain, not a parent to mount on.
		// Stopping at the first networked actor that is not ours also makes an actor mounted
		// on its own view impossible, which is the one attachment nobody can apply.
		if (AncestorOwner != nullptr && AncestorOwner != Owner)
		{
			const UCYGView* ParentView = AncestorOwner->FindComponentByClass<UCYGView>();

			if (ParentView != nullptr && ParentView->ViewId > 0)
			{
				OutViewId = ParentView->ViewId;

				return AncestorOwner->GetRootComponent();
			}
		}

		Ancestor = Ancestor->GetAttachParent();
	}

	// Nothing networked above the mount, so there is no attachment id to publish and the
	// patch says "world": the peer hangs it off the map instead of on an actor.
	return nullptr;
}

int32 UCYGAttachmentSync::GetMountedViewId() const
{
	int32 ParentViewId = 0;
	ResolveParentRoot(ParentViewId);

	return ParentViewId;
}

void UCYGAttachmentSync::Publish(int32 ParentViewId, const FVector& Location, const FQuat& Rotation, const FVector& Scale)
{
	bOutHasParent = ParentViewId > 0;
	bOutHasScale = bSyncScale;
	OutParentViewId = ParentViewId;
	OutLocation = Location;
	OutRotation = Rotation;
	OutScale = Scale;
}

void UCYGAttachmentSync::CommitBaseline(int32 ParentViewId, const FVector& Location, const FQuat& Rotation)
{
	LastParentViewId = ParentViewId;
	LastLocation = Location;
	LastRotation = Rotation;
	bHasBaseline = true;
}

void UCYGAttachmentSync::ResetAuthorityBaseline()
{
	bHasBaseline = false;
	bDirty = false;
}

bool UCYGAttachmentSync::HasCYGDelta() const
{
	return bDirty;
}

void UCYGAttachmentSync::SerializeCYG(FCYGBitWriter& Sink)
{
	// No call to the base: a Unity mount has no sync vars, so its blob starts at the flag
	// byte, and a dirty-count header in front of it would misalign every field after it.
	uint8 Flags = 0;

	if (bOutHasParent)
	{
		Flags |= ParentFlag;
	}

	if (bOutHasScale)
	{
		Flags |= ScaleFlag;
	}

	Sink.WriteUInt8(Flags);

	if (bOutHasParent)
	{
		Sink.WriteUInt32(static_cast<uint32>(OutParentViewId));
	}

	Sink.WriteVector3Packed(FCYGCoordinateSpace::ToWire(OutLocation));
	Sink.WriteQuaternion(FCYGCoordinateSpace::ToWire(OutRotation));

	if (bOutHasScale)
	{
		Sink.WriteVector3(FCYGCoordinateSpace::Relabel(OutScale));
	}

	bDirty = false;
}

void UCYGAttachmentSync::DeserializeCYG(FCYGBitReader& Source)
{
	const uint8 Flags = Source.ReadUInt8();

	const bool bHasScale = (Flags & ScaleFlag) != 0;
	const bool bHasSibling = (Flags & SiblingFlag) != 0;

	const int32 ParentViewId = (Flags & ParentFlag) != 0 ? static_cast<int32>(Source.ReadUInt32()) : 0;
	const FVector Location = FCYGCoordinateSpace::FromWire(Source.ReadVector3Packed());
	const FQuat Rotation = FCYGCoordinateSpace::FromWire(Source.ReadQuaternion());
	const FVector Scale = bHasScale ? FCYGCoordinateSpace::Unrelabel(Source.ReadVector3()) : FVector::OneVector;

	// A child order has no Unreal meaning, but the byte still has to leave the stream or
	// every component behind this one in the delta would decode from the wrong offset.
	if (bHasSibling)
	{
		Source.ReadUInt8();
	}

	// The authority decodes to keep the stream aligned and never moves its own mount.
	if (HasAuthority())
	{
		return;
	}

	ApplyOrQueue(ParentViewId, bHasScale, Location, Rotation, Scale);
}

UCYGAttachmentSync::EParentResolution UCYGAttachmentSync::ResolveParent(int32 ParentViewId, USceneComponent*& OutParent) const
{
	OutParent = nullptr;

	if (ParentViewId <= 0)
	{
		return EParentResolution::Ready;
	}

	UCYGView* ParentView = nullptr;

	if (!FCYGViewTable::TryGet(ParentViewId, ParentView) || ParentView == nullptr)
	{
		return EParentResolution::Waiting;
	}

	OutParent = ParentView->GetOwner() != nullptr ? ParentView->GetOwner()->GetRootComponent() : nullptr;

	return OutParent != nullptr ? EParentResolution::Ready : EParentResolution::Unusable;
}

void UCYGAttachmentSync::ApplyOrQueue(int32 ParentViewId, bool bHasScale, const FVector& Location, const FQuat& Rotation, const FVector& Scale)
{
	USceneComponent* Parent = nullptr;
	const EParentResolution Resolution = ResolveParent(ParentViewId, Parent);

	if (Resolution == EParentResolution::Waiting)
	{
		QueuePending(ParentViewId, bHasScale, Location, Rotation, Scale);

		return;
	}

	if (Resolution == EParentResolution::Unusable)
	{
		RejectUnusableParent(ParentViewId);

		return;
	}

	if (ApplyMount(Parent, bHasScale, Location, Rotation, Scale))
	{
		ClearPending();
	}
}

void UCYGAttachmentSync::QueuePending(int32 ParentViewId, bool bHasScale, const FVector& Location, const FQuat& Rotation, const FVector& Scale)
{
	PendingParentViewId = ParentViewId;
	bPendingHasScale = bHasScale;
	PendingLocation = Location;
	PendingRotation = Rotation;
	PendingScale = Scale;
	PendingAttempts = 0;
	bPendingGaveUpWarned = false;
	bHasPending = true;
}

void UCYGAttachmentSync::RetryPending()
{
	USceneComponent* Parent = nullptr;
	const EParentResolution Resolution = ResolveParent(PendingParentViewId, Parent);

	if (Resolution == EParentResolution::Waiting)
	{
		if (PendingAttempts < MaxPendingMountAttempts)
		{
			++PendingAttempts;

			return;
		}

		if (!bPendingGaveUpWarned)
		{
			bPendingGaveUpWarned = true;
			UE_LOG(LogCookYourGame, Warning, TEXT("%s on %s could not resolve parent view %d within %d ticks; dropping the mount patch."),
				*GetName(), *GetNameSafe(GetOwner()), PendingParentViewId, MaxPendingMountAttempts);
		}

		ClearPending();

		return;
	}

	if (Resolution == EParentResolution::Unusable)
	{
		RejectUnusableParent(PendingParentViewId);

		return;
	}

	if (ApplyMount(Parent, bPendingHasScale, PendingLocation, PendingRotation, PendingScale))
	{
		ClearPending();
	}
}

void UCYGAttachmentSync::RejectUnusableParent(int32 ParentViewId)
{
	ClearPending();

	if (bWarnedUnusableParent)
	{
		return;
	}

	bWarnedUnusableParent = true;
	UE_LOG(LogCookYourGame, Warning, TEXT("%s on %s was told to mount on view %d, which has no root component to hang from."),
		*GetName(), *GetNameSafe(GetOwner()), ParentViewId);
}

bool UCYGAttachmentSync::ApplyMount(USceneComponent* Parent, bool bHasScale, const FVector& Location, const FQuat& Rotation, const FVector& Scale)
{
	if (Target == nullptr)
	{
		return false;
	}

	if (Parent != nullptr)
	{
		if (Parent == Target || IsMountedInsideSelf(Parent))
		{
			// A cycle would leave the engine computing a world transform that depends on
			// itself. Refusing is final, so anything queued for it goes now.
			UE_LOG(LogCookYourGame, Error, TEXT("%s on %s refuses to mount inside its own actor; the patch was dropped."),
				*GetName(), *GetNameSafe(GetOwner()));
			ClearPending();

			return false;
		}

		if (Target->GetMobility() != EComponentMobility::Movable)
		{
			// The engine will not reparent something static at run time, so say which asset
			// to fix rather than let the attach fail with a generic complaint every patch.
			if (!bWarnedStaticMobility)
			{
				bWarnedStaticMobility = true;
				UE_LOG(LogCookYourGame, Warning, TEXT("%s on %s cannot mount %s, whose mobility is not Movable."),
					*GetName(), *GetNameSafe(GetOwner()), *Target->GetName());
			}

			ClearPending();

			return false;
		}

		Target->AttachToComponent(Parent, FAttachmentTransformRules::KeepRelativeTransform);
	}
	else
	{
		Target->DetachFromComponent(FDetachmentTransformRules::KeepRelativeTransform);
	}

	Target->SetRelativeLocation(Location);
	Target->SetRelativeRotation(Rotation);

	if (bHasScale)
	{
		Target->SetRelativeScale3D(Scale);
	}

	return true;
}

bool UCYGAttachmentSync::IsMountedInsideSelf(const USceneComponent* Parent) const
{
	for (const USceneComponent* Step = Parent; Step != nullptr; Step = Step->GetAttachParent())
	{
		if (Step == Target)
		{
			return true;
		}
	}

	return false;
}

void UCYGAttachmentSync::ClearPending()
{
	bHasPending = false;
	bPendingGaveUpWarned = false;
	PendingAttempts = 0;
	PendingParentViewId = 0;
}

void UCYGAttachmentSync::BindClient()
{
	UCYGClient* Current = UCYGView::ResolveClient(this);

	if (Current == BoundClient.Get())
	{
		return;
	}

	UnbindClient();

	if (Current == nullptr)
	{
		return;
	}

	DespawnHandle = Current->OnDespawnNative.AddUObject(this, &UCYGAttachmentSync::HandleDespawn);
	BoundClient = Current;
}

void UCYGAttachmentSync::UnbindClient()
{
	if (UCYGClient* Previous = BoundClient.Get())
	{
		Previous->OnDespawnNative.Remove(DespawnHandle);
	}

	DespawnHandle.Reset();
	BoundClient = nullptr;
}

void UCYGAttachmentSync::HandleDespawn(int32 ViewId)
{
	// A parent that despawned is an expected answer, not a failure: the mount belongs to
	// an actor this peer is losing anyway, so drop the patch without spending the budget.
	if (bHasPending && PendingParentViewId == ViewId)
	{
		ClearPending();
	}
}
