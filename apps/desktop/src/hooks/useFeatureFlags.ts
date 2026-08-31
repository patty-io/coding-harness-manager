import { useQuery } from "@tanstack/react-query";
import { getFeatureFlags } from "../lib/api";

/** Feature flags are app configuration, not per-screen state. Keeping one
 * query key lets Settings refresh the sidebar and route gates immediately
 * after a flag is changed. */
export function useFeatureFlags() {
  return useQuery({
    queryKey: ["feature-flags"],
    queryFn: getFeatureFlags,
    staleTime: 60_000,
  });
}
