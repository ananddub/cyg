using CookYourGame.Sim;
using UnityEngine;

namespace CookYourGame.SportsArena
{
    /// <summary>Minimal IMGUI scoreboard so the port needs no UI prefab wiring.</summary>
    public sealed class SportsArenaHud : MonoBehaviour
    {
        private GUIStyle _title;
        private GUIStyle _score;
        private GUIStyle _label;
        private GUIStyle _message;

        private void OnGUI()
        {
            SportsArenaBootstrap arena = SportsArenaBootstrap.Instance;
            if (arena == null || arena.Simulation == null)
            {
                return;
            }

            EnsureStyles();
            SportsArenaSnapshot snapshot = arena.Simulation.Snapshot;
            float width = Mathf.Min(Screen.width - 24f, 920f);
            float x = (Screen.width - width) * 0.5f;
            Rect panel = new Rect(x, 12f, width, 86f);
            GUI.color = new Color(0.02f, 0.03f, 0.05f, 0.78f);
            GUI.DrawTexture(panel, Texture2D.whiteTexture);
            GUI.color = Color.white;

            GUI.Label(new Rect(x + 18f, 16f, width * 0.55f, 26f), "CYG SPORTS ARENA", _title);
            int remaining = Mathf.Max(0, Mathf.CeilToInt(snapshot.MatchTime.AsFloat()));
            string time = (remaining / 60).ToString("00") + ":" + (remaining % 60).ToString("00");
            GUI.Label(new Rect(x + width * 0.5f - 40f, 16f, 80f, 26f), time, _title);

            GUI.Label(new Rect(x + 18f, 46f, 150f, 32f), "BLUE  " + snapshot.Scores[0], _score);
            GUI.Label(new Rect(x + width - 168f, 46f, 150f, 32f), snapshot.Scores[1] + "  RED", _score);

            for (int slot = 0; slot < SportsArenaConfig.PlayerCount / 2; slot++)
            {
                DrawHealth(x + 18f + slot * 72f, 80f, 66f, snapshot.Health[slot * 2],
                    new Color(0.15f, 0.5f, 1f));
                DrawHealth(x + width - 228f + slot * 72f, 80f, 66f, snapshot.Health[slot * 2 + 1],
                    new Color(1f, 0.25f, 0.2f));
            }

            if (!string.IsNullOrEmpty(snapshot.Message))
            {
                GUI.Label(new Rect(0f, Screen.height * 0.38f, Screen.width, 80f), snapshot.Message, _message);
            }

            string controls;
            if (arena.NetworkMode)
            {
                string role = "P" + (arena.LocalPlayerIndex + 1);
                string status = !arena.NetworkConnected
                    ? "CONNECTING"
                    : (arena.HasRemotePlayer ? "PEER ONLINE" : "WAITING FOR PEER");
                controls = "YOU ARE " + role + "   |   OpenOxide: " + status
                    + "   |   WASD move   Space jump   J punch   K dash   E throw   L block\n"
                    + "Both windows publish keyboard input through the backend input hub.";
            }
            else
            {
                controls = "P1  WASD move   Space jump   J punch   K dash   E throw   L block\n"
                    + "P2  Arrows move   R-Ctrl jump   Num1 punch   Num2 dash   Num3 throw   Num0 block";
            }
            GUI.Label(new Rect(16f, Screen.height - 68f, Screen.width - 32f, 60f), controls, _label);
        }

        private void DrawHealth(float x, float y, float width, FP health, Color color)
        {
            float fraction = Mathf.Clamp01(health.AsFloat() / SportsArenaConfig.MaxHealth.AsFloat());
            Rect background = new Rect(x, y, width, 10f);
            GUI.color = new Color(0.08f, 0.08f, 0.1f, 0.9f);
            GUI.DrawTexture(background, Texture2D.whiteTexture);
            GUI.color = color;
            GUI.DrawTexture(new Rect(x, y, width * fraction, 10f), Texture2D.whiteTexture);
            GUI.color = Color.white;
        }

        private void EnsureStyles()
        {
            if (_title != null)
            {
                return;
            }
            _title = new GUIStyle(GUI.skin.label)
            {
                fontSize = 22,
                fontStyle = FontStyle.Bold,
                alignment = TextAnchor.MiddleCenter,
                normal = { textColor = Color.white },
            };
            _score = new GUIStyle(GUI.skin.label)
            {
                fontSize = 26,
                fontStyle = FontStyle.Bold,
                alignment = TextAnchor.MiddleLeft,
                normal = { textColor = new Color(0.85f, 0.9f, 1f) },
            };
            _label = new GUIStyle(GUI.skin.label)
            {
                fontSize = 14,
                alignment = TextAnchor.MiddleCenter,
                normal = { textColor = new Color(0.85f, 0.88f, 0.95f) },
            };
            _message = new GUIStyle(GUI.skin.label)
            {
                fontSize = 42,
                fontStyle = FontStyle.Bold,
                alignment = TextAnchor.MiddleCenter,
                normal = { textColor = Color.white },
            };
        }
    }
}
