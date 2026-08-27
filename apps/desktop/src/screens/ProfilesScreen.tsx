import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";

interface ProfileView {
  id: string;
  name: string;
  harnessType: string;
  modelDisplay: string | null;
  providerName: string | null;
  roleMappings: { role: string; model: string }[];
}

export default function ProfilesScreen() {
  const qc = useQueryClient();
  const { data: profiles, isLoading } = useQuery({
    queryKey: ["profiles"],
    queryFn: () => invoke<ProfileView[]>("list_profiles_cmd"),
  });
  const del = useMutation({
    mutationFn: (id: string) => invoke<void>("delete_profile_cmd", { id }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["profiles"] }),
  });

  return (
    <div>
      <h1 className="text-2xl font-bold">Profiles</h1>
      {isLoading && <p className="mt-4">Loading…</p>}
      <ul className="mt-4 space-y-2">
        {(profiles ?? []).map((p) => (
          <li key={p.id} className="flex items-center justify-between rounded border border-gray-200 bg-white p-3 text-sm">
            <div>
              <span className="font-medium">{p.name}</span>{" "}
              <span className="text-gray-500">{p.harnessType}</span>
            </div>
            <button
              onClick={() => { if (window.confirm("Delete profile?")) del.mutate(p.id); }}
              className="rounded border border-red-200 px-2 py-0.5 text-xs text-red-600"
            >
              Delete
            </button>
          </li>
        ))}
      </ul>
      {(profiles ?? []).length === 0 && !isLoading && (
        <p className="mt-3 text-sm text-gray-500">
          No profiles yet. Create one with “create profile” in the CLI or UI.
        </p>
      )}
    </div>
  );
}
