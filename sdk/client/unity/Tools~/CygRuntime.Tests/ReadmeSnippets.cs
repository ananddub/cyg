// Compile-only: every call the README shows must exist with those arguments.
using System.Collections.Generic;
using System.Threading.Tasks;
using CYG;

namespace CYG.Tests
{
    internal static class ReadmeSnippets
    {
        internal static async Task All(string origin, string token, string serverKey)
        {
            // ---- join / lanes
            var room = await CygClient.JoinOrCreate(origin, "game_room");
            var room2 = await CygClient.JoinOrCreate(origin, "game_room", null, CygClient.Lane.Udp);
            var room3 = await CygClient.JoinOrCreate(origin, "game_room", null, CygClient.Lane.WebSocket);
            room.OnRawDelta += (delta) => { };
            room.SendInput(new byte[20]);
            room.Leave();

            // ---- recovery knobs the README documents
            room.AutoReconnect = true;
            room.MaxReconnectAttempts = 6;
            room.OnReconnecting += (attempt, delayMs) => { };
            room.OnReconnect += () => { };
            room.OnError += reason => { };

            // ---- auth
            var auth = new CygAuth(origin, serverKey);
            await auth.LoginDevice("device-id-long-enough");
            await auth.LoginEmail("a@b.c", "pw");
            await auth.RegisterEmail("a@b.c", "pw", "name");

            // ---- rest
            var rest = new CygRest(origin, auth.Token);
            double gems = await rest.GetBalanceAsync("gems");
            Dictionary<string, double> wallet = await rest.GetWalletAsync();
            var items = await rest.GetInventoryAsync();
            int arrows = await rest.GetItemCountAsync("arrow");
            await rest.ConsumeItemAsync("arrow", 1);

            await rest.DefineBoardAsync("weekly_kills", "Weekly kills", sortOrder: -1);
            await rest.SubmitScoreAsync("weekly_kills", 42);
            var top = await rest.ListScoresAsync("weekly_kills", 10);
            var mine = await rest.GetMyScoreAsync("weekly_kills");

            var quests = await rest.GetQuestsAsync();
            await rest.ClaimQuestAsync("daily_1");
            await rest.TrackEventAsync("kill", 1);
            var notes = await rest.GetNotificationsAsync(20);
            await rest.DeleteNotificationAsync("id");

            string save = await rest.ReadStorageAsync("slot_1");
            await rest.WriteStorageAsync("slot_1", "{}");
            await rest.DeleteStorageAsync("slot_1");

            await rest.VerifyAppleAsync("receipt", "product");
            await rest.VerifyGoogleAsync("receipt", "product");

            // ---- error branches the README documents
            try { await rest.GetWalletAsync(); }
            catch (CygApiException e) when (e.IsUnauthorized) { }
            catch (CygApiException e) when (e.IsNotFound) { }
            catch (CygApiException e) when (e.IsRefused) { }

            // ---- voice
            var voice = room.AttachVoice();
            voice.Enable();
            voice.SetTeam("red");
            voice.SetMuted(true);
            voice.SetDeafened(false);
            voice.OnPacket += (speakerId, packet) => { };
            voice.Send(VoiceChannel.TeamOnly, 1, 2, 0, new byte[0]);
            voice.Dispose();
        }
    }
}
