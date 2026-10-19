// Copyright CookYourGame. All rights reserved.

using UnrealBuildTool;

public class CookYourGame : ModuleRules
{
	public CookYourGame(ReadOnlyTargetRules Target) : base(Target)
	{
		PCHUsage = ModuleRules.PCHUsageMode.UseExplicitOrSharedPCHs;

		// Every dependency below is an engine module, so the plugin builds on
		// Win64, Linux, Mac, iOS and Android without linking an external binary.
		// Niagara is Epic's own particle plugin, enabled by every game template.
		PublicDependencyModuleNames.AddRange(
			new string[]
			{
				"Core",
				"CoreUObject",
				"Engine",
				"WebSockets",
				"HTTP",
				"Json",
				"Niagara"
			}
		);
	}
}
