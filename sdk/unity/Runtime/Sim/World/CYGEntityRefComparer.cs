using System.Collections.Generic;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Deterministic entity ordering: slot index first, then generation. Used by
    /// systems that keep a keyed side table and must walk it in a stable order
    /// instead of relying on dictionary bucket order.
    /// </summary>
    public sealed class CYGEntityRefComparer : IComparer<CYGEntityRef>
    {
        public static readonly CYGEntityRefComparer Instance = new CYGEntityRefComparer();

        private CYGEntityRefComparer()
        {
        }

        public int Compare(CYGEntityRef left, CYGEntityRef right)
        {
            int byIndex = left.Index.CompareTo(right.Index);
            return byIndex != 0 ? byIndex : left.Version.CompareTo(right.Version);
        }
    }
}
