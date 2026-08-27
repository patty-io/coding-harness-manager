import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  addCatalogBatch,
  createRouteCmd,
  deleteRouteCmd,
  enrichRoute,
  listCatalogAll,
  listRoutes,
  resolveEnrichment,
  updateRouteCmd,
  type EnrichOutcome,
  type RouteUpdateInput,
} from "../lib/api";

export function useRoutes() {
  return useQuery({ queryKey: ["routes"], queryFn: listRoutes });
}

export function useUpdateRoute() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, input }: { id: string; input: RouteUpdateInput }) =>
      updateRouteCmd(id, input),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["routes"] }),
  });
}

export function useDeleteRoute() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: deleteRouteCmd,
    onSuccess: () => qc.invalidateQueries({ queryKey: ["routes"] }),
  });
}

export function useCatalogAll() {
  return useQuery({ queryKey: ["catalog-all"], queryFn: listCatalogAll });
}

export function useImportBatch() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: addCatalogBatch,
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["routes"] });
      qc.invalidateQueries({ queryKey: ["catalog-all"] });
    },
  });
}

export function useCreateRoute() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: createRouteCmd,
    onSuccess: () => qc.invalidateQueries({ queryKey: ["routes"] }),
  });
}

export function useEnrich() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: enrichRoute,
    onSuccess: () => qc.invalidateQueries({ queryKey: ["routes"] }),
  });
}

export function useResolveEnrichment() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ routeId, identityId }: { routeId: string; identityId: string }) =>
      resolveEnrichment(routeId, identityId),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["routes"] }),
  });
}

export type { EnrichOutcome };