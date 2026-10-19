using System.Collections.Generic;

namespace CookYourGame
{
    /// <summary>A participant in the room, local or remote, with custom properties.</summary>
    public sealed class CYGPlayer
    {
        /// <summary>Server-assigned short id used by view ids and ownership.</summary>
        public int ActorNumber;

        /// <summary>Stable session identifier issued by the room server.</summary>
        public string PlayerId;

        /// <summary>Display name chosen at join time.</summary>
        public string Nickname;

        /// <summary>True for the player object belonging to this client.</summary>
        public bool IsLocal;

        /// <summary>True for the earliest-joined participant that owns room authority.</summary>
        public bool IsMasterClient;

        /// <summary>Free-form room properties keyed by name.</summary>
        public readonly Dictionary<string, string> CustomProperties = new Dictionary<string, string>();

        /// <summary>Reads or writes a custom property; reads return null when absent.</summary>
        public string this[string key]
        {
            get
            {
                if (string.IsNullOrEmpty(key))
                {
                    return null;
                }

                string value;
                return CustomProperties.TryGetValue(key, out value) ? value : null;
            }
            set
            {
                if (string.IsNullOrEmpty(key))
                {
                    return;
                }

                CustomProperties[key] = value;
            }
        }

        public override string ToString()
        {
            return "[CYGPlayer " + ActorNumber + " '" + Nickname + "' " + PlayerId + "]";
        }
    }
}
