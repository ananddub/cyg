// Copyright CookYourGame. All rights reserved.

using UnrealBuildTool;

public class CookYourGameEditor : ModuleRules
{
	public CookYourGameEditor(ReadOnlyTargetRules Target) : base(Target)
	{
		PCHUsage = ModuleRules.PCHUsageMode.UseExplicitOrSharedPCHs;

		PublicDependencyModuleNames.AddRange(
			new string[]
			{
				"Core"
			}
		);

		// Editor-only, and every module here ships with the engine, so the plugin
		// still links nothing it had to compile itself. Slate and Framework are both
		// listed because the dock tab moved between them across engine versions.
		PrivateDependencyModuleNames.AddRange(
			new string[]
			{
				"CookYourGame",
				"CoreUObject",
				"Engine",
				"Framework",
				"PropertyEditor",
				"Slate",
				"SlateCore"
			}
		);
	}
}
