// Copyright CookYourGame. All rights reserved.

#include "Gameplay/CYGTransformSync.h"

#include "Components/SceneComponent.h"
#include "Gameplay/CYGCoordinateSpace.h"
#include "Serialization/CYGBitPacker.h"

namespace
{
	/** A centimetre of travel, the same 0.01 m the Unity client ignores. */
	constexpr double PositionThresholdSquared = 1.0 * 1.0;
	constexpr float RotationThresholdDegrees = 0.05f;
	constexpr double ScaleThresholdSquared = 1e-6;

	/** Five metres: past this the actor was moved, not moved along. */
	constexpr double TeleportDistanceSquared = 500.0 * 500.0;

	constexpr double MinimumSampleSpanSeconds = 1e-6;
}

UCYGTransformSync::UCYGTransformSync()
{
	PrimaryComponentTick.bCanEverTick = true;
	PrimaryComponentTick.TickGroup = TG_PrePhysics;
}

void UCYGTransformSync::OnRegister()
{
	// Before Super, so the tick function is created already knowing it may run.
	PrimaryComponentTick.bCanEverTick = true;

	Super::OnRegister();

	ResolveTarget();
	RegisterAsObservable();
}

void UCYGTransformSync::ResolveTarget()
{
	if (Target == nullptr && GetOwner() != nullptr)
	{
		Target = GetOwner()->GetRootComponent();
	}
}

void UCYGTransformSync::OnSpawned()
{
	bHasSentBaseline = false;
	bHasIncomingBaseline = false;
	bPoseDirty = false;
	OutFlags = 0;
	Smooth.Reset();
}

void UCYGTransformSync::TickComponent(float DeltaTime, ELevelTick TickType, FActorComponentTickFunction* ThisTickFunction)
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

	const double NowSeconds = FPlatformTime::Seconds();

	if (HasAuthority())
	{
		if (NowSeconds >= SampleUntilSeconds)
		{
			SampleUntilSeconds = NowSeconds + FMath::Max(0.001f, SampleIntervalSeconds);
			SampleAuthority();
		}

		return;
	}

	if (Smooth.IsEmpty())
	{
		return;
	}

	ApplySmoothedPose(Smooth.Sample(NowSeconds - InterpolationDelaySeconds, MaxExtrapolationSeconds));
}

void UCYGTransformSync::SampleAuthority()
{
	const FVector Location = Target->GetComponentLocation();
	const FQuat Rotation = Target->GetComponentQuat();
	const FVector Scale = Target->GetRelativeScale3D();

	if (!bHasSentBaseline)
	{
		LastSentLocation = Location;
		LastSentRotation = Rotation;
		LastSentScale = Scale;
		bHasSentBaseline = true;

		return;
	}

	const double MoveSquared = FVector::DistanceSquared(Location, LastSentLocation);

	uint8 Flags = 0;

	if (MoveSquared >= PositionThresholdSquared)
	{
		Flags |= PositionFlag;
	}

	if (CYGPoseMath::AngleBetween(LastSentRotation, Rotation) > FMath::DegreesToRadians(RotationThresholdDegrees))
	{
		Flags |= RotationFlag;
	}

	if (bSyncScale && FVector::DistanceSquared(Scale, LastSentScale) >= ScaleThresholdSquared)
	{
		Flags |= ScaleFlag;
	}

	if (Flags == 0)
	{
		return;
	}

	if (MoveSquared >= TeleportDistanceSquared)
	{
		Flags |= TeleportFlag;
	}

	OutLocation = Location;
	OutRotation = Rotation;
	OutScale = Scale;
	LastSentLocation = Location;
	LastSentRotation = Rotation;
	LastSentScale = Scale;

	MarkPoseDirty(Flags);
}

void UCYGTransformSync::MarkPoseDirty(uint8 Flags)
{
	OutFlags |= Flags;
	bPoseDirty = true;
}

bool UCYGTransformSync::HasCYGDelta() const
{
	return bPoseDirty || UCYGBehaviour::HasCYGDelta();
}

void UCYGTransformSync::SerializeCYG(FCYGBitWriter& Sink)
{
	UCYGBehaviour::SerializeCYG(Sink);

	// The flags byte goes out even when clear, so the reader's layout matches the
	// one a sync-var-only delta produced.
	Sink.WriteUInt8(OutFlags);

	if (!bPoseDirty)
	{
		return;
	}

	if ((OutFlags & PositionFlag) != 0)
	{
		Sink.WriteVector3Packed(FCYGCoordinateSpace::ToWire(OutLocation));
	}

	if ((OutFlags & RotationFlag) != 0)
	{
		Sink.WriteQuaternion(FCYGCoordinateSpace::ToWire(OutRotation));
	}

	if ((OutFlags & ScaleFlag) != 0)
	{
		Sink.WriteVector3(FCYGCoordinateSpace::Relabel(OutScale));
	}

	OutFlags = 0;
	bPoseDirty = false;
}

void UCYGTransformSync::DeserializeCYG(FCYGBitReader& Source)
{
	UCYGBehaviour::DeserializeCYG(Source);

	const uint8 Flags = Source.ReadUInt8();

	FVector Location = bHasIncomingBaseline ? LastIncomingLocation
		: (Target != nullptr ? Target->GetComponentLocation() : FVector::ZeroVector);
	FQuat Rotation = bHasIncomingBaseline ? LastIncomingRotation
		: (Target != nullptr ? Target->GetComponentQuat() : FQuat::Identity);

	if ((Flags & PositionFlag) != 0)
	{
		Location = FCYGCoordinateSpace::FromWire(Source.ReadVector3Packed());
	}

	if ((Flags & RotationFlag) != 0)
	{
		Rotation = FCYGCoordinateSpace::FromWire(Source.ReadQuaternion());
	}

	const bool bCarriesScale = (Flags & ScaleFlag) != 0;
	const FVector Scale = bCarriesScale
		? FCYGCoordinateSpace::Unrelabel(Source.ReadVector3())
		: FVector::OneVector;

	// The authority decodes to keep the stream aligned, then keeps its own pose.
	if (HasAuthority())
	{
		return;
	}

	if (bCarriesScale && Target != nullptr)
	{
		Target->SetRelativeScale3D(Scale);
	}

	const double NowSeconds = FPlatformTime::Seconds();

	FVector Velocity = FVector::ZeroVector;

	if (bHasIncomingBaseline)
	{
		const double Span = NowSeconds - LastIncomingSeconds;

		if (Span > MinimumSampleSpanSeconds && Span < 1.0)
		{
			Velocity = (Location - LastIncomingLocation) / Span;
		}
	}

	LastIncomingLocation = Location;
	LastIncomingRotation = Rotation;
	LastIncomingSeconds = NowSeconds;
	bHasIncomingBaseline = true;

	const bool bJumped = (Flags & TeleportFlag) != 0
		|| (!Smooth.IsEmpty() && FVector::DistanceSquared(Location, Smooth.Newest().Location) >= TeleportDistanceSquared);

	FCYGTrackedPose Incoming;
	Incoming.TimeSeconds = NowSeconds;
	Incoming.Location = Location;
	Incoming.Rotation = Rotation;
	Incoming.Velocity = Velocity;

	if (bJumped || Smooth.IsEmpty())
	{
		Smooth.Restart(Incoming);
		ApplySmoothedPose(Incoming);
		OnPoseTeleported(Location, Rotation);

		return;
	}

	Smooth.Push(Incoming);
}

void UCYGTransformSync::ApplySmoothedPose(const FCYGTrackedPose& Pose)
{
	if (Target != nullptr)
	{
		Target->SetWorldLocationAndRotation(Pose.Location, Pose.Rotation);
	}
}

void UCYGTransformSync::TeleportTo(const FVector& NewLocation)
{
	if (Target == nullptr)
	{
		ResolveTarget();

		if (Target == nullptr)
		{
			return;
		}
	}

	Target->SetWorldLocation(NewLocation);
	LastSentLocation = NewLocation;

	if (HasAuthority())
	{
		OutLocation = NewLocation;
		OutRotation = Target->GetComponentQuat();
		OutScale = Target->GetRelativeScale3D();
		MarkPoseDirty(PositionFlag | TeleportFlag | (bSyncScale ? ScaleFlag : 0));

		return;
	}

	FCYGTrackedPose Anchored;
	Anchored.TimeSeconds = FPlatformTime::Seconds();
	Anchored.Location = NewLocation;
	Anchored.Rotation = Target->GetComponentQuat();

	Smooth.Restart(Anchored);
	LastIncomingLocation = NewLocation;
	LastIncomingRotation = Anchored.Rotation;
	LastIncomingSeconds = Anchored.TimeSeconds;
	bHasIncomingBaseline = true;
	ApplySmoothedPose(Anchored);
}

void UCYGTransformSync::TeleportRotation(const FQuat& NewRotation)
{
	if (Target == nullptr)
	{
		ResolveTarget();

		if (Target == nullptr)
		{
			return;
		}
	}

	Target->SetWorldRotation(NewRotation);
	LastSentRotation = NewRotation;

	if (HasAuthority())
	{
		OutLocation = Target->GetComponentLocation();
		OutRotation = NewRotation;
		OutScale = Target->GetRelativeScale3D();
		MarkPoseDirty(RotationFlag | TeleportFlag | (bSyncScale ? ScaleFlag : 0));

		return;
	}

	FCYGTrackedPose Anchored;
	Anchored.TimeSeconds = FPlatformTime::Seconds();
	Anchored.Location = Target->GetComponentLocation();
	Anchored.Rotation = NewRotation;

	Smooth.Restart(Anchored);
	LastIncomingLocation = Anchored.Location;
	LastIncomingRotation = NewRotation;
	LastIncomingSeconds = Anchored.TimeSeconds;
	bHasIncomingBaseline = true;
	ApplySmoothedPose(Anchored);
}
