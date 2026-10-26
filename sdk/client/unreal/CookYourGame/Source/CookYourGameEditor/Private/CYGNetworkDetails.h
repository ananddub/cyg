// Copyright CookYourGame. All rights reserved.

#pragma once

#include "CoreMinimal.h"
#include "IDetailCustomization.h"

class IDetailLayoutBuilder;

/**
 * Live readout above a UCYGView: the wire identity, who may write it, and which
 * components ride its deltas.
 *
 * The properties themselves are left to the default panel. What they cannot show
 * is the state a view only has while a link is up, so this adds one row that reads
 * the same accessors the send pump does, every time the editor refreshes it.
 */
class FCYGViewDetails : public IDetailCustomization
{
public:
	static TSharedRef<IDetailCustomization> MakeInstance();

	virtual void CustomizeDetails(IDetailLayoutBuilder& DetailBuilder) override;
};

/**
 * Live readout above any networked component: the view it belongs to, this side's
 * authority, and how much of its state is waiting to be sent.
 */
class FCYGBehaviourDetails : public IDetailCustomization
{
public:
	static TSharedRef<IDetailCustomization> MakeInstance();

	virtual void CustomizeDetails(IDetailLayoutBuilder& DetailBuilder) override;
};
