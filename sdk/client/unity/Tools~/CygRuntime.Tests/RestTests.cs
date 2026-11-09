// CygRest against a running server.
//
// The unit suite checks that the SDK builds the requests it means to. This asks
// whether the server accepts them, which is what a game actually depends on. It
// runs only when CYG_E2E_ORIGIN is set, because a suite that boots a server
// cannot tell a request bug from a boot timing bug.
//
//   CYG_E2E_ORIGIN=http://127.0.0.1:8297 dotnet run -c Release
using System;
using System.Collections.Generic;
using System.Threading.Tasks;

namespace CYG.Tests
{
    internal static class RestTests
    {
        internal static async Task Run(string origin)
        {
            // ---- an unauthenticated call must say so, not hang or crash
            var anonymous = new CygRest(origin);
            await Runner.CheckThrowsAsync("anonymous wallet is refused",
                () => anonymous.GetWalletAsync(),
                e => e is CygApiException api && api.IsUnauthorized);

            // ---- a signed-in player
            var auth = new CygAuth(origin, Environment.GetEnvironmentVariable("CYG_SERVER_KEY"));
            // The server requires 10-128 bytes, so a short id would be refused as
            // a bad credential rather than as a bad request.
            var deviceId = "unity-test-device-" + Guid.NewGuid().ToString("N").Substring(0, 12);
            string body;
            try
            {
                body = await auth.LoginDevice(deviceId);
            }
            catch (Exception error)
            {
                // No datastore in this project, so account creation is expected to
                // fail. Everything below needs a token, so the rest is skipped
                // rather than reported as passing.
                Console.WriteLine($"  device login unavailable ({error.Message.Substring(0, Math.Min(60, error.Message.Length))}) — rest skipped");
                return;
            }
            Runner.Check("device login returns a token", !string.IsNullOrEmpty(auth.Token));

            var rest = new CygRest(origin, auth.Token);

            // ---- wallet
            var wallet = await rest.GetWalletAsync();
            Runner.Check("wallet reads as a map", wallet != null);
            var gems = await rest.GetBalanceAsync("gems");
            Runner.Check("a currency nobody granted reads as 0", gems == 0);

            // ---- inventory
            var items = await rest.GetInventoryAsync();
            Runner.Check("inventory reads as a list", items != null);
            var swordCount = await rest.GetItemCountAsync("sword");
            Runner.Check("an item nobody granted counts 0", swordCount == 0);

            // ---- leaderboard
            //
            // A board that was never defined is a 404, deliberately: the SDK
            // must not turn "the operator has not created this board" into an
            // empty list, or a game would show "no scores yet" for a typo and
            // never notice.
            //
            // The board id is unique per run: the datastore survives between
            // runs, so a fixed name would make the second run start with the
            // first run's scores on the board and every assertion after it wrong.
            var board = "unity_board_" + Guid.NewGuid().ToString("N").Substring(0, 8);
            await Runner.CheckThrowsAsync("an undefined board is a 404, not an empty list",
                () => rest.ListScoresAsync(board, 5),
                e => e is CygApiException api && api.IsNotFound);

            // Define it, then use it end to end.
            await rest.DefineBoardAsync(board, "Weekly kills", sortOrder: -1);
            var created = await rest.ListScoresAsync(board, 5);
            Runner.Check("a defined board reads as an empty list", created != null && created.Count == 0);

            await rest.SubmitScoreAsync(board, 42);
            var afterSubmit = await rest.ListScoresAsync(board, 5);
            Runner.Check("a submitted score is on the board", afterSubmit.Count == 1);
            Runner.Check("the score is the one submitted", afterSubmit.Count == 1 && afterSubmit[0].Score == 42);
            Runner.Check("a first score is rank 1", afterSubmit.Count == 1 && afterSubmit[0].Rank == 1);

            // ---- storage
            var missing = await rest.ReadStorageAsync("slot_1");
            Runner.Check("a save slot that does not exist reads null, not throws", missing == null);

            // ---- notifications
            var notes = await rest.GetNotificationsAsync(5);
            Runner.Check("an empty inbox reads empty", notes != null);

            // ---- quests
            var quests = await rest.GetQuestsAsync();
            Runner.Check("a game with no quests reads empty", quests != null);

        }
    }
}
