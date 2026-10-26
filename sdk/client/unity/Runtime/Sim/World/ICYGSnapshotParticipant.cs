namespace CookYourGame.Sim
{
    /// <summary>
    /// Deterministic state that lives outside <see cref="CYGWorld"/> but must be
    /// rolled back and checksummed with it. Physics worlds, navigation agents,
    /// RNG streams or event journals that keep their own buffers implement this
    /// contract instead of copying themselves into components every tick.
    ///
    /// Participants are ordered by <see cref="ParticipantId"/> so registration
    /// order cannot change a checksum. Capture, restore and checksum are called
    /// on the tick path and must not allocate once their buffers have grown.
    /// </summary>
    public interface ICYGSnapshotParticipant
    {
        /// <summary>Stable id hashed into the runner checksum and fingerprint.</summary>
        uint ParticipantId { get; }

        /// <summary>Diagnostics only; never hashed.</summary>
        string ParticipantName { get; }

        /// <summary>
        /// Number of ticks the participant can restore. Either 0 for state that
        /// never rolls back (checksum-only) or at least the runner's rollback
        /// capacity.
        /// </summary>
        int SnapshotCapacity { get; }

        /// <summary>Stores the participant state at the end of <paramref name="tick"/>.</summary>
        void CaptureSnapshot(int tick);

        /// <summary>Restores the state captured for <paramref name="tick"/>.</summary>
        void RestoreSnapshot(int tick);

        /// <summary>Drops snapshots newer than <paramref name="tick"/> after a rollback.</summary>
        void DiscardSnapshotsAfter(int tick);

        /// <summary>Hashes the current participant state without allocating.</summary>
        uint ComputeChecksum();
    }
}
