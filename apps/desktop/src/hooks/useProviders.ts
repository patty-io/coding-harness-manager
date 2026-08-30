import {
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import {
  addDiscoveredToMyModels,
  checkEndpointHealth,
  createEndpoint,
  createProvider,
  deleteProvider,
  discoverEndpointModels,
  discoverProviderModels,
  envVarSet,
  listCatalogModels,
  listEndpoints,
  listProviderCatalog,
  listProviders,
  providerSummary,
  providerSummaries,
  saveApiKey,
  updateProvider,
  type EndpointInput,
} from "../lib/api";

export function useProviders() {
  return useQuery({ queryKey: ["providers"], queryFn: listProviders });
}

export function useProviderSummaries(
) {
  const query = useQuery({
    queryKey: ["provider-summaries"],
    queryFn: providerSummaries,
    staleTime: 15_000,
  });
  return {
    ...query,
    byProvider: new Map((query.data ?? []).map((entry) => [entry.providerId, entry.summary])),
  };
}

export function useCreateProvider() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ name, displayName }: { name: string; displayName: string }) =>
      createProvider(name, displayName),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["providers"] }),
  });
}

export function useUpdateProvider() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({
      id,
      displayName,
      enabled,
      notes,
    }: {
      id: string;
      displayName: string;
      enabled: boolean;
      notes: string | null;
    }) => updateProvider(id, displayName, enabled, notes),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["providers"] }),
  });
}

export function useDeleteProvider() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: deleteProvider,
    onSuccess: () => qc.invalidateQueries({ queryKey: ["providers"] }),
  });
}

export function useEndpoints(providerId: string | undefined) {
  return useQuery({
    queryKey: ["endpoints", providerId],
    queryFn: () => listEndpoints(providerId!),
    enabled: !!providerId,
  });
}

export function useCreateEndpoint() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ input, envVarName }: { input: EndpointInput; envVarName?: string }) =>
      createEndpoint(input, envVarName),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["endpoints"] }),
  });
}

export function useProviderSummary(providerId: string | undefined) {
  return useQuery({
    queryKey: ["provider-summary", providerId],
    queryFn: () => providerSummary(providerId!),
    enabled: !!providerId,
  });
}

export function useCatalog(endpointId: string | undefined) {
  return useQuery({
    queryKey: ["catalog", endpointId],
    queryFn: () => listCatalogModels(endpointId!),
    enabled: !!endpointId,
  });
}

export function useProviderCatalog(providerId: string | undefined) {
  return useQuery({
    queryKey: ["provider-catalog", providerId],
    queryFn: () => listProviderCatalog(providerId!),
    enabled: !!providerId,
  });
}

export function useCheckHealth(endpointId: string) {
  return useMutation({
    mutationFn: () => checkEndpointHealth(endpointId),
  });
}

export function useDiscover(endpointId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => discoverEndpointModels(endpointId),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["catalog", endpointId] }),
  });
}

export function useDiscoverProvider(providerId: string | undefined) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => discoverProviderModels(providerId!),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["provider-summary", providerId] });
      qc.invalidateQueries({ queryKey: ["endpoints", providerId] });
      qc.invalidateQueries({ queryKey: ["catalog"] });
      qc.invalidateQueries({ queryKey: ["provider-catalog"] });
      qc.invalidateQueries({ queryKey: ["routes"] });
    },
  });
}

export function useAddDiscoveredToMyModels() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (catalogIds: string[]) => addDiscoveredToMyModels(catalogIds),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["routes"] });
      qc.invalidateQueries({ queryKey: ["provider-summary"] });
      qc.invalidateQueries({ queryKey: ["provider-catalog"] });
    },
  });
}

export function useSaveApiKey() {
  return useMutation({
    mutationFn: ({ keyName, value }: { keyName: string; value: string }) =>
      saveApiKey(keyName, value),
  });
}

export function useEnvVarSet() {
  return useMutation({
    mutationFn: (varName: string) => envVarSet(varName),
  });
}
