import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  harnessDrift,
  listInstallations,
  recordManualSnapshot,
  revertToBaseline,
  scanHarnesses,
  type HarnessInstallation,
} from "../lib/api";

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

export function useHarnessDrift(installationId: string | undefined) {
  return useQuery({
    queryKey: ["drift", installationId],
    queryFn: () => harnessDrift(installationId!),
    enabled: !!installationId,
  });
}

export function useRecordManualSnapshot() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (installationId: string) => recordManualSnapshot(installationId),
    onSuccess: (_data, installationId) => {
      qc.invalidateQueries({ queryKey: ["drift", installationId] });
      qc.invalidateQueries({ queryKey: ["history"] });
    },
  });
}

export function useRevertToBaseline() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (installationId: string) => revertToBaseline(installationId),
    onSuccess: (_data, installationId) => {
      qc.invalidateQueries({ queryKey: ["drift", installationId] });
      qc.invalidateQueries({ queryKey: ["harness-state", installationId] });
      qc.invalidateQueries({ queryKey: ["harness-raw", installationId] });
      qc.invalidateQueries({ queryKey: ["history"] });
      qc.invalidateQueries({ queryKey: ["dashboard"] });
    },
  });
}
