// CygCoreClient.cpp — dynamic loading of libcyg_core_rs; no link-time dep.

#include "CygCoreClient.h"
#include "Misc/Paths.h"
#include "HAL/PlatformProcess.h"
#include "Containers/CString.h"

// C ABI of cyg-core-rs (crates/cyg-core-rs/src/wasm_ffi.rs + ffi.rs)
extern "C" {
    typedef uint32 (*cyg_create_fn)(const uint8*, size_t);
    typedef void (*cyg_free_fn)(uint32);
    typedef int32 (*cyg_apply_payload_fn)(uint32, const uint8*, size_t);
    typedef double (*cyg_get_f64_by_index_fn)(uint32, uint32, size_t);
    typedef int32 (*cyg_get_bool_by_index_fn)(uint32, uint32, size_t);
    typedef uint32 (*cyg_entity_count_fn)(uint32);
    typedef uint32 (*cyg_entity_id_at_fn)(uint32, uint32);
    typedef int32 (*cyg_field_index_fn)(uint32, const char*, size_t);
    typedef int32 (*cyg_state_ready_fn)(uint32);
    typedef int32 (*cyg_encode_schema_input_fn)(uint32, uint16, const int64*, size_t, uint8*, size_t, size_t*);
}

namespace
{
    struct CygFns
    {
        void* Library = nullptr;
        cyg_create_fn Create = nullptr;
        cyg_free_fn Free = nullptr;
        cyg_apply_payload_fn ApplyPayload = nullptr;
        cyg_get_f64_by_index_fn GetF64 = nullptr;
        cyg_get_bool_by_index_fn GetBool = nullptr;
        cyg_entity_count_fn EntityCount = nullptr;
        cyg_entity_id_at_fn EntityIdAt = nullptr;
        cyg_field_index_fn FieldIndex = nullptr;
        cyg_state_ready_fn StateReady = nullptr;
        cyg_encode_schema_input_fn EncodeSchemaInput = nullptr;

        bool Load()
        {
            if (Library) return true;
            const FString Base = FPaths::ProjectPluginsDir()
                / TEXT("CygCore/Source/ThirdParty/cyg-core/");
#if PLATFORM_WINDOWS
            const FString Lib = Base / TEXT("cyg_core_rs.dll");
#elif PLATFORM_MAC
            const FString Lib = Base / TEXT("libcyg_core_rs.dylib");
#else
            const FString Lib = Base / TEXT("libcyg_core_rs.so");
#endif
            Library = FPlatformProcess::GetDllHandle(*Lib);
            if (!Library) return false;
            auto Resolve = [&](const TCHAR* Name, void** Out) {
                *Out = FPlatformProcess::GetDllExport(Library, Name);
                return *Out != nullptr;
            };
            bool Ok = true;
            Ok &= Resolve(TEXT("cyg_create"), (void**)&Create);
            Ok &= Resolve(TEXT("cyg_free"), (void**)&Free);
            Ok &= Resolve(TEXT("cyg_apply_payload"), (void**)&ApplyPayload);
            Ok &= Resolve(TEXT("cyg_get_f64_by_index"), (void**)&GetF64);
            Ok &= Resolve(TEXT("cyg_get_bool_by_index"), (void**)&GetBool);
            Ok &= Resolve(TEXT("cyg_entity_count"), (void**)&EntityCount);
            Ok &= Resolve(TEXT("cyg_entity_id_at"), (void**)&EntityIdAt);
            Ok &= Resolve(TEXT("cyg_field_index"), (void**)&FieldIndex);
            Ok &= Resolve(TEXT("cyg_state_ready"), (void**)&StateReady);
            Ok &= Resolve(TEXT("cyg_encode_schema_input"), (void**)&EncodeSchemaInput);
            return Ok;
        }
    };

    CygFns& Fns()
    {
        static CygFns Fns;
        Fns.Load();
        return Fns;
    }
}

UCygCoreClient::UCygCoreClient() = default;

UCygCoreClient::~UCygCoreClient()
{
    if (CoreHandle)
    {
        if (Fns().Free) Fns().Free((uint32)(uintptr_t)CoreHandle);
        CoreHandle = nullptr;
    }
}

bool UCygCoreClient::LoadDescriptor(const TArray<uint8>& Descriptor)
{
    if (!Fns().Create || Descriptor.Num() == 0) return false;
    if (CoreHandle) { Fns().Free((uint32)(uintptr_t)CoreHandle); CoreHandle = nullptr; }
    const uint32 Handle = Fns().Create(Descriptor.GetData(), (size_t)Descriptor.Num());
    if (Handle == 0) return false;
    CoreHandle = (void*)(uintptr_t)Handle;
    return true;
}

int32 UCygCoreClient::ApplyPayload(const TArray<uint8>& Payload)
{
    if (!CoreHandle || !Fns().ApplyPayload) return -1;
    const int32 Events = Fns().ApplyPayload(
        (uint32)(uintptr_t)CoreHandle, Payload.GetData(), (size_t)Payload.Num());
    if (Events > 0) OnStateEvent.Broadcast(Events);
    return Events;
}

int32 UCygCoreClient::EntityCount() const
{
    if (!CoreHandle || !Fns().EntityCount) return 0;
    return (int32)Fns().EntityCount((uint32)(uintptr_t)CoreHandle);
}

int64 UCygCoreClient::EntityIdAt(int32 Pos) const
{
    if (!CoreHandle || !Fns().EntityIdAt) return -1;
    const uint32 Id = Fns().EntityIdAt((uint32)(uintptr_t)CoreHandle, (uint32)Pos);
    return Id == 0xFFFFFFFFu ? -1 : (int64)Id;
}

int32 UCygCoreClient::FieldIndex(const FString& Name) const
{
    if (!CoreHandle || !Fns().FieldIndex) return -1;
    const FTCHARToUTF8 Utf8(*Name);
    return Fns().FieldIndex((uint32)(uintptr_t)CoreHandle, (const char*)Utf8.Get(), (size_t)Utf8.Length());
}

double UCygCoreClient::GetF64(int64 EntityId, int32 FieldIndex_) const
{
    if (!CoreHandle || !Fns().GetF64 || FieldIndex_ < 0) return NAN;
    return Fns().GetF64((uint32)(uintptr_t)CoreHandle, (uint32)EntityId, (size_t)FieldIndex_);
}

FString UCygCoreClient::GetString(int64 EntityId, int32 FieldIndex_) const
{
    if (!CoreHandle || FieldIndex_ < 0) return FString();
    // two-pass: length probe then fill (uses cyg_get_string_by_index via a
    // small inline; for brevity the length probe + fill mirrors the C# path)
    static cyg_get_bool_by_index_fn Unused = nullptr; (void)Unused;
    if (!Fns().Library) return FString();
    auto GetStringLen = (int32(*)(uint32, uint32, size_t))
        FPlatformProcess::GetDllExport(Fns().Library, TEXT("cyg_get_string_len"));
    auto GetStringBytes = (int32(*)(uint32, uint32, size_t, uint8*, size_t))
        FPlatformProcess::GetDllExport(Fns().Library, TEXT("cyg_get_string_by_index"));
    if (!GetStringLen || !GetStringBytes) return FString();
    const int32 Len = GetStringLen((uint32)(uintptr_t)CoreHandle, (uint32)EntityId, (size_t)FieldIndex_);
    if (Len < 0) return FString();
    TArray<uint8> Buf;
    Buf.SetNumZeroed(Len);
    if (GetStringBytes((uint32)(uintptr_t)CoreHandle, (uint32)EntityId, (size_t)FieldIndex_,
                       Buf.GetData(), (size_t)Buf.Num()) < 0) return FString();
    return FString(UTF8_TO_TCHAR((const char*)Buf.GetData()));
}

bool UCygCoreClient::GetBool(int64 EntityId, int32 FieldIndex_) const
{
    if (!CoreHandle || !Fns().GetBool || FieldIndex_ < 0) return false;
    return Fns().GetBool((uint32)(uintptr_t)CoreHandle, (uint32)EntityId, (size_t)FieldIndex_) == 1;
}

bool UCygCoreClient::IsReady() const
{
    if (!CoreHandle || !Fns().StateReady) return false;
    return Fns().StateReady((uint32)(uintptr_t)CoreHandle) == 1;
}

TArray<uint8> UCygCoreClient::EncodeSchemaInput(int32 Seq, const TArray<int64>& Values) const
{
    TArray<uint8> Out;
    if (!CoreHandle || !Fns().EncodeSchemaInput) return Out;
    TArray<uint8> Buf;
    Buf.SetNumZeroed(256);
    size_t Len = 0;
    int32 Rc = Fns().EncodeSchemaInput((uint32)(uintptr_t)CoreHandle, (uint16)Seq,
        Values.GetData(), (size_t)Values.Num(), Buf.GetData(), (size_t)Buf.Num(), &Len);
    if (Rc == -2)
    {
        Buf.SetNumZeroed((int32)Len);
        Rc = Fns().EncodeSchemaInput((uint32)(uintptr_t)CoreHandle, (uint16)Seq,
            Values.GetData(), (size_t)Values.Num(), Buf.GetData(), (size_t)Buf.Num(), &Len);
    }
    if (Rc != 0) return Out;
    Out.Append(Buf.GetData(), (int32)Len);
    return Out;
}
