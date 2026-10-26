// Copyright CookYourGame. All rights reserved.

#include "Gameplay/CYGPhysicsSync.h"

#include "Components/PrimitiveComponent.h"
#include "Gameplay/CYGCoordinateSpace.h"
#include "Serialization/CYGBitPacker.h"

namespace
{
	/** Velocities are worth a byte past these, in centimetres and degrees per second. */
	constexpr double LinearThresholdSquared = 5.0 * 5.0;
	constexpr double AngularThresholdSquared = 5.0 * 5.0;

	constexpr double MinimumSampleSpanSeconds = 1e-6;

	uint8 SquaredFlags(const double Squared, const double ThresholdSquared, const uint8 Flag)
	{
		return Squared >= ThresholdSquared ? Flag : 0;
	}
}

UCYGPhysicsSync::UCYGPhysicsSync()
{
	PrimaryComponentTick.bCanEverTick = true;
	PrimaryComponentTick.TickGroup = TG_PrePhysics;
}

void UCYGPhysicsSync::OnRegister()
{
	PrimaryComponentTick.bCanEverTick = true;

	Super::OnRegister();

	ResolveBody();
	RegisterAsObservable();
}

void UCYGPhysicsSync::ResolveBody()
{
	if (Body != nullptr || GetOwner() == nullptr)
	{
		return;
	}

	TArray<UActorComponent*> Components;
	GetOwner()->GetComponents(UPrimitiveComponent::StaticClass(), Components);

	for (UActorComponent* Component : Components)
	{
		UPrimitiveComponent* Primitive = Cast<UPrimitiveComponent>(Component);

		if (Primitive != nullptr && Primitive->IsSimulatingPhysics())
		{
			Body = Primitive;

			return;
		}
	}
}

void UCYGPhysicsSync::OnSpawned()
{
	bHasSentBaseline = false;
	bPoseDirty = false;
	OutFlags = 0;
	Smooth.Reset();
}

void UCYGPhysicsSync::TickComponent(float DeltaTime, ELevelTick TickType, FActorComponentTickFunction* ThisTickFunction)
{
	Super::TickComponent(DeltaTime, TickType, ThisTickFunction);

	if (Body == nullptr)
	{
		ResolveBody();

		if (Body == nullptr)
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

	if (Smooth.IsEmpty() || !Body->IsSimulatingPhysics())
	{
		return;
	}

	FCYGTrackedPose Desired = Smooth.Sample(NowSeconds - InterpolationDelaySeconds, MaxExtrapolationSeconds);

	// Further away than the correction threshold means the local solver and the
	// authority no longer describe the same fall, so stop easing and jump.
	if (FVector::DistanceSquared(Body->GetComponentLocation(), Desired.Location)
		> CorrectionThresholdCentimetres * CorrectionThresholdCentimetres)
	{
		Desired = Smooth.Newest();
	}

	WritePose(Body, Desired);
}

void UCYGPhysicsSync::SampleAuthority()
{
	const FVector Location = Body->GetComponentLocation();
	const FQuat Rotation = Body->GetComponentQuat();
	const FVector Linear = Body->GetPhysicsLinearVelocity();
	const FVector Angular = Body->GetPhysicsAngularVelocityInDegrees();

	if (!bHasSentBaseline)
	{
		LastSentLocation = Location;
		LastSentRotation = Rotation;
		LastSentLinearVelocity = Linear;
		LastSentAngularVelocity = Angular;
		bHasSentBaseline = true;

		return;
	}

	uint8 Flags = 0;
	Flags |= SquaredFlags(FVector::DistanceSquared(Location, LastSentLocation),
		PositionThresholdCentimetres * PositionThresholdCentimetres, PositionFlag);
	Flags |= SquaredFlags(FVector::DistanceSquared(Linear, LastSentLinearVelocity), LinearThresholdSquared, LinearFlag);
	Flags |= SquaredFlags(FVector::DistanceSquared(Angular, LastSentAngularVelocity), AngularThresholdSquared, AngularFlag);

	// A body that is rotating at all is worth a sample; a resting one is not.
	if (CYGPoseMath::AngleBetween(LastSentRotation, Rotation) > FMath::DegreesToRadians(RotationThresholdDegrees))
	{
		Flags |= RotationFlag;
	}

	if (Flags == 0)
	{
		return;
	}

	OutLocation = Location;
	OutRotation = Rotation;
	OutLinearVelocity = Linear;
	OutAngularVelocity = Angular;
	LastSentLocation = Location;
	LastSentRotation = Rotation;
	LastSentLinearVelocity = Linear;
	LastSentAngularVelocity = Angular;

	OutFlags |= Flags;
	bPoseDirty = true;
}

bool UCYGPhysicsSync::HasCYGDelta() const
{
	return bPoseDirty || UCYGBehaviour::HasCYGDelta();
}

void UCYGPhysicsSync::SerializeCYG(FCYGBitWriter& Sink)
{
	UCYGBehaviour::SerializeCYG(Sink);

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

	if ((OutFlags & LinearFlag) != 0)
	{
		Sink.WriteVector3Packed(FCYGCoordinateSpace::ToWire(OutLinearVelocity));
	}

	if ((OutFlags & AngularFlag) != 0)
	{
		// A rotation rate is a pseudo-vector in axes only: degrees per second stay degrees per second.
		Sink.WriteVector3Packed(FCYGCoordinateSpace::Relabel(OutAngularVelocity));
	}

	OutFlags = 0;
	bPoseDirty = false;
}

void UCYGPhysicsSync::DeserializeCYG(FCYGBitReader& Source)
{
	UCYGBehaviour::DeserializeCYG(Source);

	const uint8 Flags = Source.ReadUInt8();

	FVector Location = Body != nullptr ? Body->GetComponentLocation() : FVector::ZeroVector;
	FQuat Rotation = Body != nullptr ? Body->GetComponentQuat() : FQuat::Identity;
	FVector Linear = Body != nullptr ? Body->GetPhysicsLinearVelocity() : FVector::ZeroVector;
	FVector Angular = Body != nullptr ? Body->GetPhysicsAngularVelocityInDegrees() : FVector::ZeroVector;

	if ((Flags & PositionFlag) != 0)
	{
		Location = FCYGCoordinateSpace::FromWire(Source.ReadVector3Packed());
	}

	if ((Flags & RotationFlag) != 0)
	{
		Rotation = FCYGCoordinateSpace::FromWire(Source.ReadQuaternion());
	}

	if ((Flags & LinearFlag) != 0)
	{
		Linear = FCYGCoordinateSpace::FromWire(Source.ReadVector3Packed());
	}

	if ((Flags & AngularFlag) != 0)
	{
		Angular = FCYGCoordinateSpace::Unrelabel(Source.ReadVector3Packed());
	}

	// The authority decodes to keep the stream aligned, then keeps its own body.
	if (HasAuthority())
	{
		return;
	}

	const double NowSeconds = FPlatformTime::Seconds();

	FVector DerivedVelocity = Linear;

	if (!Smooth.IsEmpty())
	{
		const double Span = NowSeconds - Smooth.Newest().TimeSeconds;

		if (Span > MinimumSampleSpanSeconds && Linear.IsNearlyZero())
		{
			DerivedVelocity = (Location - Smooth.Newest().Location) / Span;
		}
	}

	FCYGTrackedPose Incoming;
	Incoming.TimeSeconds = NowSeconds;
	Incoming.Location = Location;
	Incoming.Rotation = Rotation;
	Incoming.Velocity = DerivedVelocity;
	Incoming.AngularVelocity = Angular;

	if (Smooth.IsEmpty()
		|| FVector::DistanceSquared(Location, Smooth.Newest().Location) >= CorrectionThresholdCentimetres * CorrectionThresholdCentimetres)
	{
		Smooth.Restart(Incoming);
		WritePose(Body, Incoming);

		return;
	}

	Smooth.Push(Incoming);
}

void UCYGPhysicsSync::WritePose(UPrimitiveComponent* InBody, const FCYGTrackedPose& Pose) const
{
	if (InBody == nullptr || !InBody->IsSimulatingPhysics())
	{
		return;
	}

	InBody->SetWorldLocationAndRotation(Pose.Location, Pose.Rotation);
	InBody->SetPhysicsLinearVelocity(Pose.Velocity);
	InBody->SetPhysicsAngularVelocityInDegrees(Pose.AngularVelocity);
}

void UCYGPhysicsSync::TeleportBody(const FVector& NewLocation, const FQuat& NewRotation)
{
	if (Body == nullptr)
	{
		ResolveBody();

		if (Body == nullptr)
		{
			return;
		}
	}

	Body->SetWorldLocationAndRotation(NewLocation, NewRotation);
	Body->SetPhysicsLinearVelocity(FVector::ZeroVector);
	Body->SetPhysicsAngularVelocityInDegrees(FVector::ZeroVector);

	LastSentLocation = NewLocation;
	LastSentRotation = NewRotation;
	LastSentLinearVelocity = FVector::ZeroVector;
	LastSentAngularVelocity = FVector::ZeroVector;

	OutLocation = NewLocation;
	OutRotation = NewRotation;
	OutLinearVelocity = FVector::ZeroVector;
	OutAngularVelocity = FVector::ZeroVector;

	if (HasAuthority())
	{
		OutFlags |= PositionFlag | RotationFlag;
		bPoseDirty = true;

		return;
	}

	FCYGTrackedPose Anchored;
	Anchored.TimeSeconds = FPlatformTime::Seconds();
	Anchored.Location = NewLocation;
	Anchored.Rotation = NewRotation;

	Smooth.Restart(Anchored);
}
