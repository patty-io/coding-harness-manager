import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  adoptCanonicalDir,
  importSkills,
  listSkills,
} from "../lib/api";

export function useSkills() {
  return useQuery({ queryKey: ["skills"], queryFn: listSkills });
}

export function useImportSkills() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: importSkills,
    onSuccess: () => qc.invalidateQueries({ queryKey: ["skills"] }),
  });
}

export function useAdoptCanonical() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: adoptCanonicalDir,
    onSuccess: () => qc.invalidateQueries({ queryKey: ["skills"] }),
  });
}
