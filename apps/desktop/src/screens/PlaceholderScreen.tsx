export function PlaceholderScreen({ title }: { title: string }) {
  return (
    <div>
      <h1 className="text-2xl font-bold capitalize">{title}</h1>
      <p className="mt-2 text-slate-300">
        This screen is implemented in a later phase.
      </p>
    </div>
  );
}