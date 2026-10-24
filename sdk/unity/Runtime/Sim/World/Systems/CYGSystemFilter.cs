namespace CookYourGame.Sim
{
    /// <summary>
    /// Stateless entity filter. Implementations must be pure: the runner
    /// evaluates a default instance for every live entity in slot order.
    /// </summary>
    public interface ICYGSystemFilter
    {
        bool Matches(CYGWorld world, CYGEntityRef entity);
    }

    /// <summary>
    /// System base that visits every live entity matching <typeparamref name="TFilter"/>
    /// in deterministic slot order.
    /// </summary>
    public abstract class CYGSystemMainThreadFilter<TFilter> : CYGSystem
        where TFilter : struct, ICYGSystemFilter
    {
        public sealed override void OnUpdate(in CYGSystemContext context)
        {
            TFilter filter = default;
            CYGWorld world = context.World;
            int slots = world.SlotCount;
            for (int i = 0; i < slots; i++)
            {
                CYGEntityRef entity;
                if (!world.TryGetEntityAt(i, out entity))
                {
                    continue;
                }
                if (!filter.Matches(world, entity))
                {
                    continue;
                }

                UpdateEntity(in context, entity);
            }
        }

        protected abstract void UpdateEntity(in CYGSystemContext context, CYGEntityRef entity);
    }
}
