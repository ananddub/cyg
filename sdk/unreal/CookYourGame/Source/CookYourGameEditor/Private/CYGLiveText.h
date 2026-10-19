// Copyright CookYourGame. All rights reserved.

#pragma once

#include "CoreMinimal.h"

class STextBlock;

/**
 * Text that re-reads its own value, and the shared pulse that says when.
 *
 * A Slate text block only repaints when something tells it to, so an editor
 * panel showing a live number needs two halves: an attribute lambda that reads
 * the counter at paint time, and a poke that marks the widget dirty. This pairs
 * them up, so a caller that wants a readout writes one expression and nobody has
 * to remember to invalidate anything.
 *
 * The registry holds weak pointers, which means a closed tab stops being
 * refreshed on its own; the editor module's ticker is the only thing that walks
 * the list, so no widget has to own a timer of its own.
 */
class FCYGLiveText
{
public:
	/** A text block whose value comes from Reader, polled whenever Refresh runs. */
	static TSharedRef<STextBlock> Make(TFunction<FText()> Reader);

	/** Marks every registered block dirty; the editor module calls it on a ticker. */
	static void Refresh();

private:
	static TArray<TWeakPtr<STextBlock>, TInlineAllocator<16>>& Blocks();
};
