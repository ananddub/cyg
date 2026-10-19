// Copyright CookYourGame. All rights reserved.

#include "CYGNetworkDetails.h"

#include "CYGLiveText.h"
#include "Core/CYGClient.h"
#include "DetailCategoryBuilder.h"
#include "DetailLayoutBuilder.h"
#include "Gameplay/CYGAnimatorSync.h"
#include "Gameplay/CYGAttachmentSync.h"
#include "Gameplay/CYGBehaviour.h"
#include "Gameplay/CYGPhysicsSync.h"
#include "Gameplay/CYGTransformSync.h"
#include "Gameplay/CYGView.h"
#include "Widgets/Text/STextBlock.h"

namespace
{
	/** The category the components' own properties already live in. */
	const FName LiveCategoryName(TEXT("CookYourGame|Network"));

	/** How many of a behaviour's synced values are waiting for the next delta. */
	int32 CountDirtyVars(const TArray<TSharedPtr<FCYGSyncVarBase>>& Variables)
	{
		int32 Dirty = 0;

		for (const TSharedPtr<FCYGSyncVarBase>& Variable : Variables)
		{
			if (Variable.IsValid() && Variable->IsDirty())
			{
				++Dirty;
			}
		}

		return Dirty;
	}

	FString LinkWords(const UCYGClient* Client)
	{
		if (Client == nullptr)
		{
			return TEXT("no room client");
		}

		return FString::Printf(TEXT("link %s   rtt %.1f ms   actor %d"),
			Client->IsLinkConnected() ? TEXT("up") : TEXT("down"),
			Client->GetLatencyMilliseconds(),
			Client->GetLocalActorNumber());
	}

	FText DescribeView(const TWeakObjectPtr<UCYGView>& Target)
	{
		UCYGView* View = Target.Get();

		if (!IsValid(View))
		{
			return FText::GetEmpty();
		}

		FString Line = FString::Printf(TEXT("view %d   owner %d   %s   authority %s\n    %s"),
			View->ViewId,
			View->OwnerActor,
			View->IsMine() ? TEXT("mine") : TEXT("not mine"),
			View->HasAuthority() ? TEXT("yes") : TEXT("no"),
			*LinkWords(View->GetClient()));

		if (View->IsSceneObject())
		{
			// A scene object is led by whoever is master client, which is the one
			// authority rule a designer reading the panel would not guess from OwnerActor.
			Line += TEXT("   scene object");
		}

		int32 Written = 0;

		for (const TObjectPtr<UObject>& Candidate : View->ObservedComponents)
		{
			UObject* Component = Candidate.Get();

			if (Component == nullptr)
			{
				continue;
			}

			UCYGBehaviour* Behaviour = Cast<UCYGBehaviour>(Component);

			if (Behaviour == nullptr)
			{
				Line += FString::Printf(TEXT("\n    %s   delta source"), *Component->GetName());
				++Written;
				continue;
			}

			// The index order here is the wire order, so listing them in this order tells
			// a reader which bit of the dirty mask each component owns.
			Line += FString::Printf(TEXT("\n    %d. %s   %s   vars %d   dirty %d"),
				Written,
				*Component->GetClass()->GetName(),
				Behaviour->HasCYGDelta() ? TEXT("delta ready") : TEXT("quiet"),
				Behaviour->SyncVars().Num(),
				CountDirtyVars(Behaviour->SyncVars()));

			++Written;
		}

		if (Written >= UCYGView::MaxObservedComponents)
		{
			Line += FString::Printf(TEXT("\n    mask is full at %d components"), UCYGView::MaxObservedComponents);
		}

		return FText::FromString(Line);
	}

	FText DescribeBehaviour(const TWeakObjectPtr<UCYGBehaviour>& Target)
	{
		UCYGBehaviour* Behaviour = Target.Get();

		if (!IsValid(Behaviour))
		{
			return FText::GetEmpty();
		}

		const UCYGView* View = Behaviour->GetView();

		FString Line = FString::Printf(TEXT("actor %s   view %s   authority %s   mine %s\n    vars %d   dirty %d   %s"),
			*GetNameSafe(Behaviour->GetOwner()),
			View != nullptr ? *FString::Printf(TEXT("%d (owner %d)"), View->ViewId, View->OwnerActor) : TEXT("unbound"),
			Behaviour->HasAuthority() ? TEXT("yes") : TEXT("no"),
			Behaviour->IsMine() ? TEXT("yes") : TEXT("no"),
			Behaviour->SyncVars().Num(),
			CountDirtyVars(Behaviour->SyncVars()),
			*LinkWords(View != nullptr ? View->GetClient() : nullptr));

		if (const UCYGAnimatorSync* Animator = Cast<UCYGAnimatorSync>(Behaviour))
		{
			// Triggers are not polled, so a parameter count is the only number here that
			// says anything about how wide this actor's animation stream is.
			Line += FString::Printf(TEXT("\n    mesh %s   parameters %d   dead zone %.5f"),
				*GetNameSafe(Animator->Mesh.Get()),
				Animator->Parameters.Num(),
				Animator->FloatDeadZone);
		}
		else if (const UCYGPhysicsSync* Physics = Cast<UCYGPhysicsSync>(Behaviour))
		{
			Line += FString::Printf(TEXT("\n    body %s   move %.1f cm   turn %.1f deg"),
				*GetNameSafe(Physics->Body.Get()),
				Physics->PositionThresholdCentimetres,
				Physics->RotationThresholdDegrees);
		}
		else if (const UCYGTransformSync* Pose = Cast<UCYGTransformSync>(Behaviour))
		{
			Line += FString::Printf(TEXT("\n    target %s   sample %.0f ms   draw %.0f ms late"),
				*GetNameSafe(Pose->GetTargetComponent()),
				Pose->SampleIntervalSeconds * 1000.0,
				Pose->InterpolationDelaySeconds * 1000.0);
		}
		else if (const UCYGAttachmentSync* Mount = Cast<UCYGAttachmentSync>(Behaviour))
		{
			// Which actor the mount is riding is the answer worth seeing: a mount reading
			// view 0 is one the room will hang off the map rather than on anything.
			Line += FString::Printf(TEXT("\n    target %s   mounted on view %d   scale %s"),
				*GetNameSafe(Mount->GetTargetComponent()),
				Mount->GetMountedViewId(),
				Mount->bSyncScale ? TEXT("sent") : TEXT("local only"));
		}

		return FText::FromString(Line);
	}

	/** One row at the top of the component's own category, reading live on refresh. */
	void AddLiveRow(IDetailLayoutBuilder& DetailBuilder, const TSharedRef<STextBlock>& Block)
	{
		FDetailCategoryBuilder& Category = DetailBuilder.AddCategory(LiveCategoryName);

		Category.AddCustomRow(FName("CYGLiveReadout"), Block);
	}
}

TSharedRef<IDetailCustomization> FCYGViewDetails::MakeInstance()
{
	return MakeShared<FCYGViewDetails>();
}

void FCYGViewDetails::CustomizeDetails(IDetailLayoutBuilder& DetailBuilder)
{
	TArray<UObject*> Targets;
	DetailBuilder.GetObjectsBeingCustomized(Targets);

	for (UObject* Object : Targets)
	{
		UCYGView* View = Cast<UCYGView>(Object);

		if (View == nullptr)
		{
			continue;
		}

		const TWeakObjectPtr<UCYGView> Weak = View;

		// The panel can outlive the component it was built for, so the reader holds a
		// weak reference and says nothing once the object is gone.
		AddLiveRow(DetailBuilder, FCYGLiveText::Make([Weak]() { return DescribeView(Weak); }));
		return;
	}
}

TSharedRef<IDetailCustomization> FCYGBehaviourDetails::MakeInstance()
{
	return MakeShared<FCYGBehaviourDetails>();
}

void FCYGBehaviourDetails::CustomizeDetails(IDetailLayoutBuilder& DetailBuilder)
{
	TArray<UObject*> Targets;
	DetailBuilder.GetObjectsBeingCustomized(Targets);

	for (UObject* Object : Targets)
	{
		UCYGBehaviour* Behaviour = Cast<UCYGBehaviour>(Object);

		if (Behaviour == nullptr)
		{
			continue;
		}

		const TWeakObjectPtr<UCYGBehaviour> Weak = Behaviour;

		AddLiveRow(DetailBuilder, FCYGLiveText::Make([Weak]() { return DescribeBehaviour(Weak); }));
		return;
	}
}
