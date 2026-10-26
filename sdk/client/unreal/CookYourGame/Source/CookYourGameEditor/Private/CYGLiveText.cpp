// Copyright CookYourGame. All rights reserved.

#include "CYGLiveText.h"

#include "Widgets/Text/STextBlock.h"

TArray<TWeakPtr<STextBlock>, TInlineAllocator<16>>& FCYGLiveText::Blocks()
{
	// Built on first use rather than as a file static, because a static container of
	// weak pointers needs its own shutdown ordering against Slate teardown.
	static TArray<TWeakPtr<STextBlock>, TInlineAllocator<16>> Registered;

	return Registered;
}

TSharedRef<STextBlock> FCYGLiveText::Make(TFunction<FText()> Reader)
{
	const TSharedRef<STextBlock> Block = SNew(STextBlock)
		.Text_Lambda(MoveTemp(Reader))
		.AutoWrapText(false);

	Blocks().Add(Block);

	return Block;
}

void FCYGLiveText::Refresh()
{
	TArray<TWeakPtr<STextBlock>, TInlineAllocator<16>>& Registered = Blocks();

	for (int32 Index = Registered.Num() - 1; Index >= 0; --Index)
	{
		const TSharedPtr<STextBlock> Block = Registered[Index].Pin();

		if (!Block.IsValid())
		{
			// A closed tab leaves its blocks behind, and this is the only pass that
			// runs often enough to be worth cleaning up in.
			Registered.RemoveAt(Index);
			continue;
		}

		// The values are read inside the attribute lambdas, so all a refresh has to
		// say is that what was painted is no longer current.
		Block->InvalidateLayoutAndVolatility();
	}
}
