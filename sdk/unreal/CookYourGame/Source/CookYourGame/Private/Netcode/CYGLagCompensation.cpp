// Copyright CookYourGame. All rights reserved.

#include "Netcode/CYGLagCompensation.h"

#include "CookYourGame.h"
#include "Components/PrimitiveComponent.h"
#include "Core/CYGClient.h"
#include "Core/CYGOpCodes.h"
#include "Gameplay/CYGCoordinateSpace.h"
#include "Netcode/CYGRewindMath.h"
#include "Serialization/CYGBitPacker.h"

namespace
{
	/** Capture ceiling, so history stays the same size whatever the project's tick rate is. */
	constexpr double CaptureIntervalSeconds = 1.0 / 60.0;

	/** Recorded bodies and their history, pruned as bodies go away. */
	TMap<TWeakObjectPtr<UPrimitiveComponent>, TWeakObjectPtr<UCYGRollbackBuffer>> Index;
}

double FCYGLagCompensation::NowOffsetSeconds = 0.0;

UCYGRollbackBuffer::UCYGRollbackBuffer()
{
	PrimaryComponentTick.bCanEverTick = true;
	PrimaryComponentTick.TickGroup = TG_PrePhysics;
}

void UCYGRollbackBuffer::OnRegister()
{
	PrimaryComponentTick.bCanEverTick = true;

	Super::OnRegister();

	ResolveBody();

	Samples.SetNum(FMath::Max(2, Capacity));
	Head = 0;
	Num = 0;
	LastCaptureSeconds = 0.0;

	if (Body != nullptr)
	{
		FCYGLagCompensation::RegisterBuffer(Body, this);
	}
}

void UCYGRollbackBuffer::OnUnregister()
{
	FCYGLagCompensation::UnregisterBuffer(Body);

	Samples.Reset();
	Head = 0;
	Num = 0;

	Super::OnUnregister();
}

void UCYGRollbackBuffer::ResolveBody()
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

		if (Primitive != nullptr && Primitive->GetCollisionEnabled() != ECollisionEnabled::NoCollision)
		{
			Body = Primitive;

			return;
		}
	}
}

void UCYGRollbackBuffer::TickComponent(float DeltaTime, ELevelTick TickType, FActorComponentTickFunction* ThisTickFunction)
{
	Super::TickComponent(DeltaTime, TickType, ThisTickFunction);

	if (Body == nullptr)
	{
		ResolveBody();

		if (Body == nullptr)
		{
			SetComponentTickEnabled(false);

			return;
		}
	}

	const double NowSeconds = FPlatformTime::Seconds() + FCYGLagCompensation::NowOffsetSeconds;

	if (NowSeconds - LastCaptureSeconds < CaptureIntervalSeconds)
	{
		return;
	}

	LastCaptureSeconds = NowSeconds;
	CaptureNow();
}

void UCYGRollbackBuffer::CaptureNow()
{
	const FBoxSphereBounds Bounds = Body->Bounds;

	FCYGRollbackSnapshot& Slot = Samples[Head];
	Slot.TimeSeconds = LastCaptureSeconds;
	Slot.Center = Bounds.Origin;
	Slot.Extents = bTreatAsSphere ? FVector(Bounds.SphereRadius, Bounds.SphereRadius, Bounds.SphereRadius) : Bounds.BoxExtent;
	Slot.bIsSphere = bTreatAsSphere;

	Head = (Head + 1) % Samples.Num();

	if (Num < Samples.Num())
	{
		++Num;
	}
}

bool UCYGRollbackBuffer::TrySampleAt(double Seconds, FCYGRollbackSnapshot& OutSample) const
{
	OutSample = FCYGRollbackSnapshot();

	if (Num == 0 || Samples.Num() == 0)
	{
		return false;
	}

	const int32 Oldest = (Head - Num + Samples.Num()) % Samples.Num();
	const int32 Newest = (Head + Samples.Num() - 1) % Samples.Num();

	// Rewinding past what was recorded would be guesswork, while a moment a little
	// newer than the last capture is only a tick behind the live clock, so it clamps.
	if (Seconds < Samples[Oldest].TimeSeconds)
	{
		return false;
	}

	if (Seconds >= Samples[Newest].TimeSeconds)
	{
		OutSample = Samples[Newest];
		OutSample.TimeSeconds = Seconds;

		return true;
	}

	int32 Second = Oldest;

	for (int32 Step = 0; Step < Num; ++Step)
	{
		const int32 Candidate = (Oldest + Step) % Samples.Num();

		if (Samples[Candidate].TimeSeconds >= Seconds)
		{
			Second = Candidate;

			break;
		}
	}

	if (Second == Oldest)
	{
		OutSample = Samples[Oldest];
		OutSample.TimeSeconds = Seconds;

		return true;
	}

	const int32 First = (Second + Samples.Num() - 1) % Samples.Num();
	const double Span = Samples[Second].TimeSeconds - Samples[First].TimeSeconds;
	const double T = Span <= 0.0 ? 0.0 : (Seconds - Samples[First].TimeSeconds) / Span;

	OutSample.TimeSeconds = Seconds;
	OutSample.Center = Samples[First].Center + (Samples[Second].Center - Samples[First].Center) * T;
	OutSample.Extents = Samples[First].Extents + (Samples[Second].Extents - Samples[First].Extents) * T;
	OutSample.bIsSphere = Samples[First].bIsSphere || Samples[Second].bIsSphere;

	return true;
}

uint8 UCYGRollbackBuffer::CollisionChannel() const
{
	return Body != nullptr ? static_cast<uint8>(Body->GetCollisionObjectType()) : 0;
}

void FCYGLagCompensation::RegisterBuffer(UPrimitiveComponent* Body, UCYGRollbackBuffer* Buffer)
{
	if (Body != nullptr && Buffer != nullptr)
	{
		Index.Add(Body, Buffer);
	}
}

void FCYGLagCompensation::UnregisterBuffer(UPrimitiveComponent* Body)
{
	if (Body != nullptr)
	{
		Index.Remove(Body);
	}
}

void FCYGLagCompensation::ClearBuffers()
{
	Index.Reset();
}

int32 FCYGLagCompensation::BufferCount()
{
	return Index.Num();
}

bool FCYGLagCompensation::RaycastRewind(const FVector& Origin, const FVector& Direction, double MaxDistance,
	double ClientSeconds, uint64 ChannelMask, const AActor* IgnoreActor, FCYGRewindHit& OutHit)
{
	OutHit = FCYGRewindHit();

	const double NowSeconds = FPlatformTime::Seconds() + NowOffsetSeconds;

	if (ClientSeconds > NowSeconds || NowSeconds - ClientSeconds > MaxRewindWindowSeconds)
	{
		return false;
	}

	const FVector Unit = Direction.GetSafeNormal();

	if (Unit.IsNearlyZero() || MaxDistance <= 0.0)
	{
		return false;
	}

	double BestDistance = TNumericLimits<double>::Max();
	FCYGRewindHit Best;

	for (auto It = Index.CreateIterator(); It; ++It)
	{
		UPrimitiveComponent* Body = It.Key().Get();
		UCYGRollbackBuffer* Buffer = It.Value().Get();

		if (Body == nullptr || Buffer == nullptr || !Body->IsRegistered())
		{
			It.RemoveCurrent();

			continue;
		}

		if (IgnoreActor != nullptr && Body->GetOwner() == IgnoreActor)
		{
			continue;
		}

		if (ChannelMask != 0 && (ChannelMask & (1ull << Buffer->CollisionChannel())) == 0)
		{
			continue;
		}

		FCYGRollbackSnapshot Sample;

		if (!Buffer->TrySampleAt(ClientSeconds, Sample))
		{
			continue;
		}

		double Distance = 0.0;
		FVector Normal = FVector::ZeroVector;
		bool bHit = false;

		if (Sample.bIsSphere)
		{
			bHit = CYGRewindMath::TryRaySphere(Origin, Unit, Sample.Center, Sample.Extents.X, MaxDistance, Distance, Normal);
		}
		else
		{
			bHit = CYGRewindMath::TryRayBox(Origin, Unit, Sample.Center, Sample.Extents, MaxDistance, Distance, Normal);
		}

		if (bHit && Distance < BestDistance)
		{
			BestDistance = Distance;
			Best.Location = Origin + Unit * Distance;
			Best.Normal = Normal;
			Best.Distance = Distance;
			Best.TimeSeconds = Sample.TimeSeconds;
			Best.Actor = Body->GetOwner();
		}
	}

	if (!Best.Actor.IsValid())
	{
		return false;
	}

	OutHit = Best;

	return true;
}

bool FCYGLagCompensation::SubmitHitClaim(UCYGClient* Client, double ClientSeconds, const FVector& ShotOrigin,
	const FVector& ShotDirection, int32 TargetViewId)
{
	if (Client == nullptr || !Client->IsLinkConnected())
	{
		return false;
	}

	const double NowSeconds = FPlatformTime::Seconds() + NowOffsetSeconds;

	if (NowSeconds - ClientSeconds > MaxRewindWindowSeconds)
	{
		UE_LOG(LogCookYourGame, Warning, TEXT("A hit claim at %.3f is %.3fs old, past the rewind window."), ClientSeconds, NowSeconds - ClientSeconds);

		return false;
	}

	// The clock travels as a single: it only ever needs to resolve to the frame it was
	// fired on, and a double would cost eight bytes on every shot.
	FCYGBitWriter Packet;
	Packet.WriteSingle(static_cast<float>(ClientSeconds));
	Packet.WriteVector3(FCYGCoordinateSpace::ToWire(ShotOrigin));
	Packet.WriteVector3(FCYGCoordinateSpace::Relabel(ShotDirection));
	Packet.WriteInt32(TargetViewId);

	return Client->SendPacket(ECYGOpCode::RewindHit, Packet.Data());
}

bool FCYGLagCompensation::TryReadClaim(const uint8* Payload, int32 Size, FCYGHitClaim& OutClaim)
{
	OutClaim = FCYGHitClaim();

	FCYGBitReader Reader(Payload, Size);

	OutClaim.TimeSeconds = static_cast<double>(Reader.ReadSingle());
	OutClaim.Origin = FCYGCoordinateSpace::FromWire(Reader.ReadVector3());
	OutClaim.Direction = FCYGCoordinateSpace::Unrelabel(Reader.ReadVector3());
	OutClaim.TargetViewId = Reader.ReadInt32();

	if (Reader.HasError())
	{
		UE_LOG(LogCookYourGame, Warning, TEXT("A hit claim was truncated after %d bytes."), Reader.BytesRead());

		return false;
	}

	return true;
}
