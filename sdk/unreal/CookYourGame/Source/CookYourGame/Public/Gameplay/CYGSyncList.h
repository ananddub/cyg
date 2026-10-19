// Copyright CookYourGame. All rights reserved.

#pragma once

#include "CoreMinimal.h"
#include "Gameplay/CYGSyncVar.h"

/** The edits a synced list remembers until the next pack. */
enum class ECYGSyncListOp : uint8
{
	Add		= 0,
	Insert	= 1,
	Set		= 2,
	RemoveAt= 3,
	Clear	= 4,
};

/**
 * A collection whose edits travel as operations rather than as a whole copy, so
 * a ten-thousand entry list costs one entry per change.
 *
 * Wire block, one per pack: [byte changeCount] then per change
 *   [byte op]
 *   add, insert, set  - [int32 index][item in the item codec's encoding]
 *   remove            - [int32 index]
 *   clear             - nothing
 *
 * Operations are applied in order on the far side, so a peer that missed none of
 * them converges on the same content. A peer that joined mid-session hydrates
 * with SetSilent instead, since it has no history to replay.
 */
template <typename Type>
class FCYGSyncList final : public FCYGSyncVarBase
{
public:
	using FAdded = TFunction<void(int32 /*Index*/, const Type& /*Item*/)>;
	using FRemoved = TFunction<void(int32 /*Index*/, const Type& /*Item*/)>;
	using FUpdated = TFunction<void(int32 /*Index*/, const Type& /*Old*/, const Type& /*New*/)>;
	using FCleared = TFunction<void()>;

	FCYGSyncList() = default;
	explicit FCYGSyncList(const TArray<Type>& InItems) : Items(InItems) {}

	int32 Num() const { return Items.Num(); }
	bool IsEmpty() const { return Items.Num() == 0; }
	const TArray<Type>& AsArray() const { return Items; }

	const Type& operator[](int32 Index) const { return Items[Index]; }

	Type& At(int32 Index) { return Items[Index]; }

	/** Replaces the whole list without replicating anything: join-time hydration. */
	void SetSilent(const TArray<Type>& InItems)
	{
		Items = InItems;
		Pending.Reset();
	}

	void Add(const Type& Item)
	{
		const int32 Index = Items.Num();
		Items.Add(Item);
		Record(ECYGSyncListOp::Add, Index, Item);

		if (AddedHook)
		{
			AddedHook(Index, Item);
		}
	}

	/** Puts an item at Index; anything from there on shifts up. */
	void Insert(int32 Index, const Type& Item)
	{
		if (Index < 0 || Index > Items.Num())
		{
			return;
		}

		Items.Insert(Item, Index);
		Record(ECYGSyncListOp::Insert, Index, Item);

		if (AddedHook)
		{
			AddedHook(Index, Item);
		}
	}

	void Set(int32 Index, const Type& Item)
	{
		if (Index < 0 || Index >= Items.Num())
		{
			return;
		}

		const Type Old = Items[Index];
		Items[Index] = Item;
		Record(ECYGSyncListOp::Set, Index, Item);

		if (UpdatedHook)
		{
			UpdatedHook(Index, Old, Item);
		}
	}

	void RemoveAt(int32 Index)
	{
		if (Index < 0 || Index >= Items.Num())
		{
			return;
		}

		const Type Old = Items[Index];
		Items.RemoveAt(Index);
		Record(ECYGSyncListOp::RemoveAt, Index, Type());

		if (RemovedHook)
		{
			RemovedHook(Index, Old);
		}
	}

	bool Remove(const Type& Item)
	{
		const int32 Index = Items.IndexOfByKey(Item);

		if (Index == INDEX_NONE)
		{
			return false;
		}

		RemoveAt(Index);
		return true;
	}

	void Clear()
	{
		Items.Reset();
		Record(ECYGSyncListOp::Clear, 0, Type());

		if (ClearedHook)
		{
			ClearedHook();
		}
	}

	bool Contains(const Type& Item) const { return Items.Contains(Item); }

	void OnItemAdded(FAdded Hook) { AddedHook = MoveTemp(Hook); }
	void OnItemRemoved(FRemoved Hook) { RemovedHook = MoveTemp(Hook); }
	void OnItemUpdated(FUpdated Hook) { UpdatedHook = MoveTemp(Hook); }
	void OnCleared(FCleared Hook) { ClearedHook = MoveTemp(Hook); }

	virtual bool IsDirty() const override
	{
		if (Pending.Num() == 0)
		{
			return false;
		}

		if (SendIntervalSeconds > 0.f && FPlatformTime::Seconds() - LastSentSeconds < SendIntervalSeconds)
		{
			return false;
		}

		return true;
	}

	virtual void PackDirty(FCYGBitWriter& Target) override
	{
		// One byte counts the changes, so a longer backlog packs in several passes.
		const int32 Count = FMath::Min(Pending.Num(), 255);

		Target.WriteUInt8(static_cast<uint8>(Count));

		for (int32 Index = 0; Index < Count; ++Index)
		{
			const FChange& Change = Pending[Index];
			Target.WriteUInt8(static_cast<uint8>(Change.Op));

			if (Change.Op == ECYGSyncListOp::Clear)
			{
				continue;
			}

			Target.WriteInt32(Change.Index);

			if (Change.Op != ECYGSyncListOp::RemoveAt)
			{
				CYGSyncCodecs::Pack<Type>(Target, Change.Item);
			}
		}

		Pending.RemoveAt(0, Count, EAllowShrinking::No);
		LastSentSeconds = FPlatformTime::Seconds();
	}

	virtual void ApplyRemote(FCYGBitReader& Source) override
	{
		const uint8 Count = Source.ReadUInt8();

		for (uint8 Step = 0; Step < Count && !Source.HasError(); ++Step)
		{
			const ECYGSyncListOp Op = static_cast<ECYGSyncListOp>(Source.ReadUInt8());

			if (Op == ECYGSyncListOp::Clear)
			{
				Items.Reset();

				if (ClearedHook)
				{
					ClearedHook();
				}

				continue;
			}

			const int32 Index = Source.ReadInt32();
			const bool bCarriesItem = Op != ECYGSyncListOp::RemoveAt;
			const Type Item = bCarriesItem ? CYGSyncCodecs::Unpack<Type>(Source) : Type();

			// An index the peer no longer has means the two lists already drifted, so
			// appending keeps the item rather than dropping the message on the floor.
			switch (Op)
			{
			case ECYGSyncListOp::Add:
			case ECYGSyncListOp::Insert:
				if (Index >= 0 && Index <= Items.Num())
				{
					Items.Insert(Item, Index);
				}
				else
				{
					Items.Add(Item);
				}

				if (AddedHook)
				{
					AddedHook(Index, Item);
				}
				break;

			case ECYGSyncListOp::Set:
				if (Index >= 0 && Index < Items.Num())
				{
					const Type Old = Items[Index];
					Items[Index] = Item;

					if (UpdatedHook)
					{
						UpdatedHook(Index, Old, Item);
					}
				}
				break;

			case ECYGSyncListOp::RemoveAt:
				if (Index >= 0 && Index < Items.Num())
				{
					const Type Old = Items[Index];
					Items.RemoveAt(Index);

					if (RemovedHook)
					{
						RemovedHook(Index, Old);
					}
				}
				break;

			default:
				break;
			}
		}
	}

private:
	struct FChange
	{
		ECYGSyncListOp Op = ECYGSyncListOp::Clear;
		int32 Index = 0;
		Type Item {};
	};

	void Record(ECYGSyncListOp Op, int32 Index, const Type& Item)
	{
		Pending.Add(FChange{ Op, Index, Item });
	}

	TArray<Type> Items;
	TArray<FChange> Pending;

	FAdded AddedHook;
	FRemoved RemovedHook;
	FUpdated UpdatedHook;
	FCleared ClearedHook;

	double LastSentSeconds = 0.0;
};
