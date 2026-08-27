import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { listInstallations, scanHarnesses, type HarnessInstallation } from "../lib/api";

export function useInstallations() {
  return useQuery({ queryKey: ["installations"], queryFn: listInstallations });
}

export function useScanHarnesses() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: scanHarnesses,
    onSuccess: (data: HarnessInstallation[]) =>
      qc.setQueryData(["installations"], data),
  });
}