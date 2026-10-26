using System;
using UnityEngine;

namespace CookYourGame
{
    /// <summary>
    /// In-memory capture of a room seat (roomId + sessionId + reconnection token) plus a bounded,
    /// windowed reconnect loop so a dropped WebSocket resumes the same seat instead of re-joining as
    /// a new player. Nothing is persisted across app restarts; the cookie lives only in this domain
    /// and never emits the reconnection token in any log line or in ToString.
    ///
    /// DRIVE MODEL: attempts are paced by <see cref="Tick"/>, fed Time.deltaTime every frame by a
    /// self-bootstrapped driver MonoBehaviour. It deliberately does NOT use a coroutine and does NOT
    /// rely on CYGNetworkManager.OnFixedNetworkTick / OnVariableUpdate, because those only fire while
    /// CYGNetwork.IsConnected and therefore cannot drive a reconnect while the link is down.
    /// </summary>
    public sealed class CYGSessionCookie
    {
        const float InitialBackoffSeconds = 0.5f;
        const int InvalidGamePort = 0;

        static string roomId = string.Empty;
        static string sessionId = string.Empty;
        static string token = string.Empty;
        static string processId = string.Empty;

        static int gamePort;
        static bool hasCookie;
        static bool reconnecting;
        static bool reconnectInFlight;
        static int attempts;
        static float windowElapsed;
        static float nextAttemptDelay;
        static float currentBackoff = InitialBackoffSeconds;

        static ResumeDriver driver;

        /// <summary>Total wall time (seconds) the resume window stays open from the disconnect moment.</summary>
        public static float WindowSeconds = 30f;

        /// <summary>Cap on the exponential backoff between attempts (seconds).</summary>
        public static float MaxBackoffSeconds = 4f;

        /// <summary>Hard ceiling on resume attempts even while the window still has time.</summary>
        public static int MaxAttempts = 6;

        /// <summary>True when a seat has been captured and not yet cleared.</summary>
        public static bool HasCookie
        {
            get { return hasCookie; }
        }

        /// <summary>Captured room id of the seat, empty when none.</summary>
        public static string RoomId
        {
            get { return roomId; }
        }

        /// <summary>Captured session id of the seat, empty when none.</summary>
        public static string SessionId
        {
            get { return sessionId; }
        }

        /// <summary>Captured reconnection token. Read-only and never emitted by any log or ToString.</summary>
        public static string Token
        {
            get { return token; }
        }

        /// <summary>True while the bounded resume loop is pacing attempts.</summary>
        public static bool IsReconnecting
        {
            get { return reconnecting; }
        }

        /// <summary>Seconds of the resume window consumed so far.</summary>
        public static float WindowElapsed
        {
            get { return windowElapsed; }
        }

        /// <summary>Seconds until the current backoff interval elapses; 0 when idle.</summary>
        public static float NextAttemptIn
        {
            get { return reconnecting ? nextAttemptDelay : 0f; }
        }

        /// <summary>Resume attempts made so far in this disconnect.</summary>
        public static int Attempts
        {
            get { return attempts; }
        }

        private CYGSessionCookie()
        {
        }

        /// <summary>
        /// Captures the seat identity and marks it fresh for a later resume. Feed the reconnection
        /// token here: CYGNetwork.seatToken is private, so the matchmake / handshake parse sites must
        /// call this to give the cookie its own copy.
        /// </summary>
        public static void Capture(string room, string session, string reconnectToken)
        {
            roomId = room ?? string.Empty;
            sessionId = session ?? string.Empty;
            token = reconnectToken ?? string.Empty;
            hasCookie = true;
            ResetAttemptState();
        }

        /// <summary>Captures the seat identity including the game process id and port for a faithful resume URL.</summary>
        public static void Capture(string room, string session, string reconnectToken, string process, int port)
        {
            Capture(room, session, reconnectToken);
            processId = process ?? string.Empty;
            gamePort = port;
        }

        /// <summary>
        /// Captures what the SDK already exposes (RoomName + SessionId) and keeps any stored token.
        /// NOTE: the reconnection token is NOT reachable from outside CYGNetwork (seatToken is private),
        /// so it is only filled when the integration owner calls Capture(room, session, token) at the
        /// seat-token parse sites, or exposes a public getter on CYGNetwork for it (see report).
        /// </summary>
        public static void CaptureFromNetwork()
        {
            roomId = CYGNetwork.RoomName ?? string.Empty;
            sessionId = CYGNetwork.SessionId ?? string.Empty;
            hasCookie = true;
            ResetAttemptState();

            if (string.IsNullOrEmpty(token))
            {
                Debug.LogWarning("[CYG] session cookie captured without a reconnection token: feed one via CYGSessionCookie.Capture at the seat token site.");
            }
        }

        /// <summary>Drops the captured seat and stops any resume loop (call on an explicit user disconnect / room leave).</summary>
        public static void Clear()
        {
            hasCookie = false;
            reconnecting = false;
            reconnectInFlight = false;
            attempts = 0;
            windowElapsed = 0f;
            nextAttemptDelay = 0f;
            currentBackoff = InitialBackoffSeconds;
            roomId = string.Empty;
            sessionId = string.Empty;
            token = string.Empty;
            processId = string.Empty;
            gamePort = InvalidGamePort;
        }

        /// <summary>
        /// Attempts one immediate resume when a fresh cookie exists, the link is down, the window is
        /// open and we are not already mid-backoff. Returns true only if a reconnect was initiated.
        /// </summary>
        public static bool TryResume()
        {
            if (!ShouldAttempt())
            {
                return false;
            }

            CYGServerSettings settings = CYGNetwork.CurrentSettings;
            if (settings == null)
            {
                GiveUp("no CYGServerSettings are available to rebuild the game socket url.");
                return false;
            }

            attempts++;
            reconnectInFlight = true;
            string url = settings.GameSocketUrl(processId, roomId, sessionId, token, gamePort);
            if (string.IsNullOrEmpty(url))
            {
                reconnectInFlight = false;
                Debug.LogWarning("[CYG] resume attempt " + attempts + " could not build a game socket url.");
                return false;
            }

            Debug.LogWarning("[CYG] attempting to resume seat in room \"" + Safe(roomId) + "\" (attempt " + attempts + "/" + MaxAttempts + ").");
            CYGNetwork.Connect(url, null);
            return true;
        }

        /// <summary>
        /// Paces the resume loop. Normally driven automatically by the bootstrapped driver; call it
        /// yourself only if you disable the driver, and then from an Update that runs while disconnected.
        /// </summary>
        public static void Tick(float deltaTime)
        {
            if (!reconnecting || reconnectInFlight || deltaTime <= 0f)
            {
                return;
            }

            windowElapsed += deltaTime;
            if (windowElapsed >= WindowSeconds)
            {
                GiveUp("the resume window expired after " + WindowSeconds + "s.");
                return;
            }

            nextAttemptDelay -= deltaTime;
            if (nextAttemptDelay > 0f)
            {
                return;
            }

            if (!TryResume())
            {
                // A failed attempt still advances the backoff schedule toward the cap.
                ScheduleNextBackoff();
            }
        }

        /// <summary>
        /// Arms or advances the resume loop; wired to CYGNetwork.OnDisconnectedEvent, and callable
        /// directly (for example by a headless test) to exercise the backoff schedule.
        /// </summary>
        public static void NotifyDisconnected(string reason)
        {
            if (!hasCookie)
            {
                return;
            }

            if (!reconnecting)
            {
                // First drop of a live session: open the window and wait the initial backoff.
                ResetAttemptState();
                reconnecting = true;
                nextAttemptDelay = currentBackoff;
            }
            else
            {
                // A resume attempt we fired failed asynchronously: keep the window running and back off further.
                reconnectInFlight = false;
                ScheduleNextBackoff();
            }
        }

        /// <summary>Masked one-line view of the cookie that never contains the reconnection token.</summary>
        public static string Describe()
        {
            if (!hasCookie)
            {
                return "[CYGSessionCookie empty]";
            }

            return "[CYGSessionCookie room=\"" + Safe(roomId) + "\" session=\"" + Safe(sessionId) + "\" attempts=" + attempts + "]";
        }

        public override string ToString()
        {
            return Describe();
        }

        [RuntimeInitializeOnLoadMethod(RuntimeInitializeLoadType.BeforeSceneLoad)]
        static void Boot()
        {
            Clear();

            // WHY -= then +=: a domain reload re-runs Boot, and the event delegates may still hold the old handlers.
            CYGNetwork.OnDisconnectedEvent -= HandleDisconnected;
            CYGNetwork.OnDisconnectedEvent += HandleDisconnected;
            CYGNetwork.OnJoinedRoom -= HandleJoinedRoom;
            CYGNetwork.OnJoinedRoom += HandleJoinedRoom;
            CYGNetwork.OnConnectedEvent -= HandleConnected;
            CYGNetwork.OnConnectedEvent += HandleConnected;

            EnsureDriver();
        }

        static void HandleDisconnected(string reason)
        {
            NotifyDisconnected(reason);
        }

        static void HandleJoinedRoom(string room)
        {
            // A confirmed (re)join is success: the seat is live again, so retire the resume loop.
            reconnectInFlight = false;
            Clear();
        }

        static void HandleConnected()
        {
            reconnectInFlight = false;
        }

        static bool ShouldAttempt()
        {
            return hasCookie && !reconnectInFlight && !CYGNetwork.IsConnected && windowElapsed < WindowSeconds && attempts < MaxAttempts;
        }

        static void ResetAttemptState()
        {
            reconnecting = false;
            reconnectInFlight = false;
            attempts = 0;
            windowElapsed = 0f;
            nextAttemptDelay = 0f;
            currentBackoff = InitialBackoffSeconds;
        }

        static void ScheduleNextBackoff()
        {
            currentBackoff = NextBackoff(currentBackoff);
            nextAttemptDelay = currentBackoff;

            if (attempts >= MaxAttempts)
            {
                GiveUp("the maximum of " + MaxAttempts + " resume attempts was reached.");
            }
        }

        static void GiveUp(string why)
        {
            Debug.LogWarning("[CYG] seat resume abandoned: " + why);
            Clear();
        }

        static float NextBackoff(float current)
        {
            float doubled = current * 2f;
            // Capped exponential backoff: 0.5 -> 1 -> 2 -> 4, clamped to the ceiling.
            return doubled > MaxBackoffSeconds ? MaxBackoffSeconds : doubled;
        }

        static string Safe(string value)
        {
            return string.IsNullOrEmpty(value) ? "(none)" : value;
        }

        static void EnsureDriver()
        {
            if (driver != null)
            {
                return;
            }

            GameObject go = new GameObject("[CYGSessionCookie]");
            go.hideFlags = HideFlags.None;
            UnityEngine.Object.DontDestroyOnLoad(go);
            driver = go.AddComponent<ResumeDriver>();
        }

        /// <summary>Hidden, always-on driver that forwards Time.deltaTime into Tick while disconnected.</summary>
        sealed class ResumeDriver : MonoBehaviour
        {
            void Update()
            {
                Tick(Time.deltaTime);
            }
        }

        [RuntimeInitializeOnLoadMethod(RuntimeInitializeLoadType.SubsystemRegistration)]
        static void ResetDomain()
        {
            Clear();
            if (driver != null)
            {
                if (driver.gameObject != null)
                {
                    UnityEngine.Object.Destroy(driver.gameObject);
                }
                driver = null;
            }
        }
    }
}
