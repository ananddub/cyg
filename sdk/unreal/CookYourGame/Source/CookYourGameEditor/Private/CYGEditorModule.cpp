// Copyright CookYourGame. All rights reserved.

#include "CYGEditorModule.h"

#include "CYGLiveText.h"
#include "CYGNetworkDetails.h"
#include "CYGNetworkProfilerWidget.h"
#include "Containers/Ticker.h"
#include "Framework/Docking/TabManager.h"
#include "Gameplay/CYGAnimatorSync.h"
#include "Gameplay/CYGAttachmentSync.h"
#include "Gameplay/CYGBehaviour.h"
#include "Gameplay/CYGCharacterMovementSync.h"
#include "Gameplay/CYGOwnershipToggle.h"
#include "Gameplay/CYGPhysicsSync.h"
#include "Gameplay/CYGServerToggle.h"
#include "Gameplay/CYGTransformSync.h"
#include "Gameplay/CYGView.h"
#include "Modules/ModuleManager.h"
#include "PropertyEditorModule.h"
#include "Widgets/Docking/SDockTab.h"

#define LOCTEXT_NAMESPACE "CYGEditor"

namespace
{
	/** Tab key, and the name the window manager files the entry under. */
	const TCHAR* const ProfilerTabName = TEXT("CYGNetworkProfiler");

	/** How often every live readout on screen is told to repaint. */
	const float LiveRefreshSeconds = 0.25f;
}

void FCYGEditorModule::StartupModule()
{
	FGlobalTabmanager::Get()
		->RegisterNomadTabSpawner(ProfilerTabName, FOnOpenTab::CreateStatic(&FCYGEditorModule::OpenProfilerTab))
		.SetDisplayName(LOCTEXT("ProfilerTab", "CookYourGame Traffic"))
		.SetTooltipText(LOCTEXT("ProfilerTabTip", "Room link traffic: bytes, packets, per-opcode totals and pump rate."));

	// One ticker for every live widget, rather than one per widget: a details panel
	// can be rebuilt dozens of times a minute and each rebuild would otherwise leave
	// a timer behind until its row happened to be pruned.
	LiveHandle = FTSTicker::GetCoreTicker().AddTicker(
		FTickerDelegate::CreateLambda([](float)
		{
			FCYGLiveText::Refresh();
			return true;
		}),
		LiveRefreshSeconds);

	RegisterDetails();
}

void FCYGEditorModule::ShutdownModule()
{
	FTSTicker::GetCoreTicker().RemoveTicker(LiveHandle);

	if (FModuleManager::Get().IsModuleLoaded(TEXT("PropertyEditor")))
	{
		if (FPropertyEditorModule* PropertyModule = FModuleManager::GetModulePtr<FPropertyEditorModule>(TEXT("PropertyEditor")))
		{
			for (const FName Layout : RegisteredLayouts)
			{
				PropertyModule->UnregisterCustomClassLayout(Layout);
			}
		}
	}

	RegisteredLayouts.Reset();

	FGlobalTabmanager::Get()->UnregisterNomadTabSpawner(ProfilerTabName);
}

void FCYGEditorModule::RegisterDetails()
{
	FPropertyEditorModule& PropertyModule = FModuleManager::LoadModuleChecked<FPropertyEditorModule>(TEXT("PropertyEditor"));

	const FOnGetDetailCustomizationInstance BehaviourLayout = FOnGetDetailCustomizationInstance::CreateStatic(&FCYGBehaviourDetails::MakeInstance);
	const FOnGetDetailCustomizationInstance ViewLayout = FOnGetDetailCustomizationInstance::CreateStatic(&FCYGViewDetails::MakeInstance);

	// Registered per class instead of relying on the layout lookup to walk the super
	// chain, because a derived component then decides which of the two readouts it
	// gets, and that is not worth leaving to chance.
	const TArray<FName> BehaviourClasses
	{
		UCYGBehaviour::StaticClass()->GetFName(),
		UCYGTransformSync::StaticClass()->GetFName(),
		UCYGCharacterMovementSync::StaticClass()->GetFName(),
		UCYGPhysicsSync::StaticClass()->GetFName(),
		UCYGAnimatorSync::StaticClass()->GetFName(),
		UCYGAttachmentSync::StaticClass()->GetFName(),
		UCYGOwnershipToggle::StaticClass()->GetFName(),
		UCYGServerToggle::StaticClass()->GetFName()
	};

	RegisteredLayouts.Reset();
	RegisteredLayouts.Append(BehaviourClasses);

	for (const FName Layout : BehaviourClasses)
	{
		PropertyModule.RegisterCustomClassLayout(Layout, BehaviourLayout);
	}

	const FName ViewName = UCYGView::StaticClass()->GetFName();
	RegisteredLayouts.Add(ViewName);
	PropertyModule.RegisterCustomClassLayout(ViewName, ViewLayout);

	PropertyModule.NotifyCustomizationModuleChanged();
}

TSharedRef<SDockTab> FCYGEditorModule::OpenProfilerTab()
{
	return SNew(SDockTab)
		.TabRole(ETabRole::NomadTab)
		[
			SNew(SCYGNetworkProfiler)
		];
}

IMPLEMENT_MODULE(FCYGEditorModule, CookYourGameEditor)

#undef LOCTEXT_NAMESPACE
