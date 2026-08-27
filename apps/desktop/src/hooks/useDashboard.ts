import { useQuery } from "@tanstack/react-query";
import { dashboardStats } from "../lib/api";

export function useDashboardStats() {
  return useQuery({ queryKey: ["dashboard"], queryFn: dashboardStats, staleTime: 15_000 });
}
