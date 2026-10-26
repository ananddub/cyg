// Copyright CookYourGame. All rights reserved.

#pragma once

#include "Containers/Ticker.h"
#include "CoreMinimal.h"
#include "Modules/ModuleManager.h"

class SDockTab;

/**
 * The editor half of the plugin: a traffic tab and the details-panel readouts for
 * the synced components.
 *
 * Nothing in here is a UObject, so the module contributes no generated headers of
 * its own, and it reaches the runtime only through the public surface of the
 * CookYourGame module. The two halves it wires up are the same two the game could
 * wire up by hand, which is the point: the panel reads the numbers a game would
 * read, not a copy of them kept for the editor's benefit.
 */
class FCYGEditorModule : public IModuleInterface
{
public:
	virtual void StartupModule() override;

	virtual void ShutdownModule() override;

private:
	/** Puts the live rows onto the components, by class name. */
	void RegisterDetails();

	/** The tab body, handed to the window manager whenever the tab is opened. */
	static TSharedRef<SDockTab> OpenProfilerTab();

	/** Class names this module took a layout for, so it can hand them back. */
	TArray<FName> RegisteredLayouts;

	/** The one pulse every live readout on screen shares. */
	FTSTicker::FDelegateHandle LiveHandle;
};
