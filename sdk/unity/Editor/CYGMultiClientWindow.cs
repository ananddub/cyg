using System;
using System.Diagnostics;
using CookYourGame.Sim;
using UnityEditor;
using UnityEngine;

namespace CookYourGame.EditorTools
{
    /// <summary>
    /// In-editor multi-client runner. Steps two to four independent
    /// deterministic worlds in lockstep and shows checksums, positions and the
    /// first divergence. Optionally launches standalone player instances for a
    /// real network test against the same build.
    /// </summary>
    public sealed class CYGMultiClientWindow : EditorWindow
    {
        private int _clientCount = 2;
        private int _playersPerClient = 2;
        private int _ticks = 300;
        private int _seed = 2026;
        private bool _autoStep;
        private float _stepInterval = 0.02f;
        private double _nextStepTime;
        private CYGMultiClientMatch _match;
        private Vector2 _scroll;

        private int _launchCount = 2;
        private string _buildPath = string.Empty;
        private string _additionalArgs = string.Empty;

        [MenuItem("CookYourGame/Sim/Multi-Client Runner")]
        public static void Open()
        {
            CYGMultiClientWindow window = GetWindow<CYGMultiClientWindow>("CYG Multi-Client");
            window.minSize = new Vector2(520f, 420f);
            window.Show();
        }

        private void OnEnable()
        {
            EditorApplication.update += OnEditorUpdate;
            if (_match == null)
            {
                CreateMatch();
            }
        }

        private void OnDisable()
        {
            EditorApplication.update -= OnEditorUpdate;
        }

        private void OnEditorUpdate()
        {
            if (!_autoStep || _match == null || _match.IsComplete)
            {
                return;
            }

            double now = EditorApplication.timeSinceStartup;
            if (now < _nextStepTime)
            {
                return;
            }

            _nextStepTime = now + Mathf.Max(0.001f, _stepInterval);
            _match.StepOnce();
            Repaint();
        }

        private void OnGUI()
        {
            EditorGUILayout.LabelField("In-Process Deterministic Clients", EditorStyles.boldLabel);
            EditorGUILayout.HelpBox(
                "Each client owns an independent CYGWorld, system runner and rollback runner. "
                + "All clients receive the same bot input stream, so any checksum difference is a determinism bug.",
                MessageType.Info);

            EditorGUI.BeginChangeCheck();
            _clientCount = EditorGUILayout.IntSlider("Clients", _clientCount, 2, 4);
            _playersPerClient = EditorGUILayout.IntSlider("Players / client", _playersPerClient, 1, 4);
            _ticks = EditorGUILayout.IntSlider("Ticks", _ticks, 1, 2000);
            _seed = EditorGUILayout.IntField("Seed", _seed);
            if (EditorGUI.EndChangeCheck())
            {
                _autoStep = false;
                CreateMatch();
            }

            EditorGUILayout.BeginHorizontal();
            if (GUILayout.Button("New Match"))
            {
                CreateMatch();
            }
            if (GUILayout.Button("Step"))
            {
                _match.StepOnce();
            }
            if (GUILayout.Button("Run All"))
            {
                _match.RunToEnd();
            }
            _autoStep = GUILayout.Toggle(_autoStep, "Auto", "Button", GUILayout.Width(60f));
            EditorGUILayout.EndHorizontal();

            if (_match == null)
            {
                return;
            }

            _scroll = EditorGUILayout.BeginScrollView(_scroll);
            EditorGUILayout.LabelField(
                "Tick " + _match.Tick + " / " + _match.TotalTicks
                + "   " + (_match.IsComplete ? "complete" : "running")
                + "   clients " + _match.ClientCount);

            if (_match.Diverged)
            {
                EditorGUILayout.HelpBox(_match.Divergence, MessageType.Error);
            }
            else if (_match.IsComplete)
            {
                EditorGUILayout.HelpBox("All clients agree on every sampled tick.", MessageType.Info);
            }

            for (int i = 0; i < _match.ClientCount; i++)
            {
                CYGMultiClientStatus status = _match.GetStatus(i);
                EditorGUILayout.BeginVertical(EditorStyles.helpBox);
                EditorGUILayout.LabelField(
                    "Client " + status.Client
                    + "   checksum 0x" + status.Checksum.ToString("X8")
                    + "   entities " + status.EntityCount
                    + "   pos " + status.Position.x.AsFloat().ToString("F2")
                    + ", " + status.Position.z.AsFloat().ToString("F2"),
                    EditorStyles.boldLabel);
                DrawMiniMap(status);
                EditorGUILayout.EndVertical();
            }
            EditorGUILayout.EndScrollView();

            EditorGUILayout.Space(8f);
            EditorGUILayout.LabelField("Standalone Instances", EditorStyles.boldLabel);
            _buildPath = EditorGUILayout.TextField("Build / Player executable", _buildPath);
            _launchCount = EditorGUILayout.IntSlider("Instances", _launchCount, 2, 4);
            _additionalArgs = EditorGUILayout.TextField("Extra args", _additionalArgs);
            using (new EditorGUI.DisabledScope(string.IsNullOrEmpty(_buildPath)))
            {
                if (GUILayout.Button("Launch " + _launchCount + " Player Instances"))
                {
                    LaunchInstances();
                }
            }
        }

        private void CreateMatch()
        {
            _match = new CYGMultiClientMatch(new CYGMultiClientOptions
            {
                ClientCount = _clientCount,
                PlayersPerClient = _playersPerClient,
                Ticks = _ticks,
                Seed = (ulong)Mathf.Max(0, _seed),
            });
            _nextStepTime = 0d;
        }

        private static void DrawMiniMap(CYGMultiClientStatus status)
        {
            Rect area = GUILayoutUtility.GetRect(0f, 54f, GUILayout.ExpandWidth(true));
            EditorGUI.DrawRect(area, new Color(0.12f, 0.13f, 0.15f, 1f));
            float centerX = area.x + area.width * 0.5f;
            float centerY = area.y + area.height * 0.5f;
            float x = centerX + status.Position.x.AsFloat() * 12f;
            float y = centerY - status.Position.z.AsFloat() * 12f;
            Rect dot = new Rect(x - 4f, y - 4f, 8f, 8f);
            EditorGUI.DrawRect(dot, status.Alive ? new Color(0.3f, 0.9f, 0.5f) : new Color(0.9f, 0.3f, 0.3f));
        }

        private void LaunchInstances()
        {
            for (int i = 0; i < _launchCount; i++)
            {
                string args = "-cygClientIndex " + i + " " + _additionalArgs;
                try
                {
                    Process.Start(new ProcessStartInfo
                    {
                        FileName = _buildPath,
                        Arguments = args,
                        UseShellExecute = true,
                    });
                }
                catch (Exception exception)
                {
                    UnityEngine.Debug.LogError(
                        "[CYG] Failed to launch player instance " + i + ": " + exception.Message);
                    return;
                }
            }
        }
    }
}
