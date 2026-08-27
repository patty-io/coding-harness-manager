import { useMutation, useQuery } from "@tanstack/react-query";
import {
  importHarnessState,
  readHarnessState,
  type ImportOptions,
} from "../lib/importApi";

export function useReadHarnessState(installationId: string | null) {
  return useQuery({
    queryKey: ["harness-state", installationId],
    queryFn: () => readHarnessState(installationId!),
    enabled: !!installationId,
  });
}

export function useImportHarnessState(installationId: string) {
  return useMutation({
    mutationFn: (options: ImportOptions) =>
      importHarnessState(installationId, options),
  });
}