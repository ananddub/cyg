// CygCoreClient.h — Unreal Engine 5 client binding over cyg-core-rs.
//
// Plugin layout (copy sdk/client/unreal/ into <Project>/Plugins/CygCore/):
//   Plugins/CygCore/Source/CygCore/
//     CygCore.Build.cs
//     Public/CygCoreClient.h
//     Private/CygCoreClient.cpp
//   and the prebuilt native lib:
//     Plugins/CygCore/Source/ThirdParty/cyg-core/Lib/... (see Build.cs)
//
// Usage (any Blueprint-visible class or subsystem):
//   UCygCoreClient* Room = NewObject<UCygCoreClient>(this);
//   Room->LoadDescriptor(DescriptorBytes);          // cyg.schema.bin
//   Room->ApplyPayload(FrameBytes);                 // from a WebSocket/UDP lane
//   float X = Room->GetF64(EntityId, Room->FieldIndex(TEXT("x")));
//
// Transport stays UE-side (WebSocket module / raw sockets); all protocol
// logic is cyg-core-rs — the same core as web/Unity/Godot.

#pragma once

#include "CoreMinimal.h"
#include "CygCoreClient.generated.h"

DECLARE_DYNAMIC_MULTICAST_DELEGATE_OneParam(FCygOnStateEvent, int32, EventCount);

UCLASS()
class CYGCORE_API UCygCoreClient : public UObject
{
    GENERATED_BODY()

public:
    UCygCoreClient();
    virtual ~UCygCoreClient() override;

    /** Parse cyg.schema.bin bytes. Returns false on a bad descriptor. */
    UFUNCTION(BlueprintCallable, Category = "Cyg")
    bool LoadDescriptor(const TArray<uint8>& Descriptor);

    /** Apply one raw room-data payload; returns event count, -1 on error. */
    UFUNCTION(BlueprintCallable, Category = "Cyg")
    int32 ApplyPayload(const TArray<uint8>& Payload);

    /** Entity count in the primary collection. */
    UFUNCTION(BlueprintCallable, Category = "Cyg")
    int32 EntityCount() const;

    /** Entity id at logical position (ascending id order); -1 = none. */
    UFUNCTION(BlueprintCallable, Category = "Cyg")
    int64 EntityIdAt(int32 Pos) const;

    /** Schema-field index by name; -1 = not found. */
    UFUNCTION(BlueprintCallable, Category = "Cyg")
    int32 FieldIndex(const FString& Name) const;

    /** f64 view of one entity field by index; NaN = absent. */
    UFUNCTION(BlueprintCallable, Category = "Cyg")
    double GetF64(int64 EntityId, int32 FieldIndex) const;

    /** String field by index; empty = absent/not a string. */
    UFUNCTION(BlueprintCallable, Category = "Cyg")
    FString GetString(int64 EntityId, int32 FieldIndex) const;

    /** Bool field by index; false = absent/not a bool. */
    UFUNCTION(BlueprintCallable, Category = "Cyg")
    bool GetBool(int64 EntityId, int32 FieldIndex) const;

    /** Client ready (kind-1 full snapshot applied)? */
    UFUNCTION(BlueprintCallable, Category = "Cyg")
    bool IsReady() const;

    /** Encode a schema-lane input frame (values are raw i64 bit patterns). */
    UFUNCTION(BlueprintCallable, Category = "Cyg")
    TArray<uint8> EncodeSchemaInput(int32 Seq, const TArray<int64>& Values) const;

    /** Fired after each successful ApplyPayload with the event count. */
    UPROPERTY(BlueprintAssignable, Category = "Cyg")
    FCygOnStateEvent OnStateEvent;

private:
    void* CoreHandle = nullptr; // opaque cyg-core handle (uint32 slot index)
};
