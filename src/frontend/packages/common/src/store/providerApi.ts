import { createApi } from "@reduxjs/toolkit/query/react";
import { createGrpcBaseQuery } from "../api";
import type { ListProvidersResponse } from "@kf2/proto/gen/provider_pb.js";
import { ProviderService } from "@kf2/proto/gen/provider_pb.js";

export const providerApi = createApi({
  reducerPath: "providerApi",
  baseQuery: createGrpcBaseQuery(),
  tagTypes: ["ProviderCatalog"],
  endpoints: (build) => ({
    listProviders: build.query<ListProvidersResponse, void>({
      query: () => ({
        service: ProviderService,
        method: "listProviders",
        input: {},
      }),
      providesTags: ["ProviderCatalog"],
    }),
  }),
});

export const { useListProvidersQuery } = providerApi;
