import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  createMcp,
  deleteMcp,
  listMcp,
  runMcpDiagnostics,
  type McpInput,
} from "../lib/api";

export function useMcpServers() {
  return useQuery({ queryKey: ["mcp"], queryFn: listMcp });
}

export function useCreateMcp() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (input: McpInput) => createMcp(input),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["mcp"] });
      void qc.invalidateQueries({ queryKey: ["detected-mcp"] });
    },
  });
}

export function useDeleteMcp() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: deleteMcp,
    onSuccess: () => qc.invalidateQueries({ queryKey: ["mcp"] }),
  });
}

export function useRunDiagnostics() {
  return useMutation({
    mutationFn: runMcpDiagnostics,
  });
}
