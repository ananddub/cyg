// CygCore.Build.cs — UE5 module rules for the CygCore client plugin.

using UnrealBuildTool;

public class CygCore : ModuleRules
{
    public CygCore(ReadOnlyTargetRules Target) : base(Target)
    {
        PCHUsage = PCHUsageMode.UseExplicitOrSharedPCHs;

        PublicDependencyModuleNames.AddRange(new string[]
        {
            "Core",
            "CoreUObject",
            "Engine",
            "WebSocket",   // transport stays UE-side; the protocol is cyg-core
        });

        // Prebuilt cyg-core native lib: build with
        //   cargo build -p cyg-core-rs --release
        // and copy the artifact into Source/ThirdParty/cyg-core/
        string CygLibDir = Path.Combine(
            ModuleDirectory, "..", "ThirdParty", "cyg-core");

        if (Target.Platform == UnrealTargetPlatform.Win64)
        {
            PublicAdditionalLibraries.Add(Path.Combine(CygLibDir, "cyg_core_rs.lib"));
            RuntimeDependencies.Add(Path.Combine(CygLibDir, "cyg_core_rs.dll"));
        }
        else if (Target.Platform == UnrealTargetPlatform.Mac)
        {
            PublicAdditionalLibraries.Add(Path.Combine(CygLibDir, "libcyg_core_rs.dylib"));
        }
        else if (Target.Platform == UnrealTargetPlatform.Linux)
        {
            PublicAdditionalLibraries.Add(Path.Combine(CygLibDir, "libcyg_core_rs.so"));
            RuntimeDependencies.Add(Path.Combine(CygLibDir, "libcyg_core_rs.so"));
        }
    }
}
