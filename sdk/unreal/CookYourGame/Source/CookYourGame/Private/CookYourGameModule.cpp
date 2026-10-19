// Copyright CookYourGame. All rights reserved.

#include "CookYourGame.h"
#include "Modules/ModuleManager.h"

DEFINE_LOG_CATEGORY(LogCookYourGame);

class FCookYourGameModule : public IModuleInterface
{
public:
	virtual void StartupModule() override
	{
		UE_LOG(LogCookYourGame, Verbose, TEXT("CookYourGame module loaded."));
	}

	virtual void ShutdownModule() override
	{
	}
};

IMPLEMENT_MODULE(FCookYourGameModule, CookYourGame)
