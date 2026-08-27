import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  importHarnessState,
  readHarnessState,
  type ImportOptions,
} from "../lib/api";

export function useReadHarnessState(installationId: string | null) {
  return useQuery({
    queryKey: ["harness-state", installationId],
    queryFn: () => readHarnessState(installationId!),
    enabled: !!installationId,
  });
}

export function useImportHarnessState() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({
      installationId,
      options,
    }: {
      installationId: string;
      options: ImportOptions;
    }) => importHarnessState(installationId, options),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["dashboard"] }),
  });
}