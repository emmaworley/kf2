import type { MessageInitShape } from "@bufbuild/protobuf";
import { createApi } from "@reduxjs/toolkit/query/react";
import { createGrpcBaseQuery } from "../api";
import type {
  ConfigureProviderResponse,
  GetProviderStatusResponse,
  ListConfiguredProvidersResponse,
  UnconfigureProviderResponse,
} from "@kf2/proto/gen/session_pb.js";
import {
  ConfigureProviderRequestSchema,
  GetProviderStatusRequestSchema,
  ListConfiguredProvidersRequestSchema,
  SessionService,
  UnconfigureProviderRequestSchema,
} from "@kf2/proto/gen/session_pb.js";

export const sessionProviderApi = createApi({
  reducerPath: "sessionProviderApi",
  baseQuery: createGrpcBaseQuery(),
  tagTypes: ["SessionProviderStatus"],
  endpoints: (build) => ({
    listConfiguredProviders: build.query<
      ListConfiguredProvidersResponse,
      MessageInitShape<typeof ListConfiguredProvidersRequestSchema>
    >({
      query: (input) => ({
        service: SessionService,
        method: "listConfiguredProviders",
        input,
      }),
      providesTags: (_result, _error, arg) => [
        { type: "SessionProviderStatus", id: arg.sessionId },
      ],
    }),

    getProviderStatus: build.query<
      GetProviderStatusResponse,
      MessageInitShape<typeof GetProviderStatusRequestSchema>
    >({
      query: (input) => ({
        service: SessionService,
        method: "getProviderStatus",
        input,
      }),
      providesTags: (_result, _error, arg) => [
        { type: "SessionProviderStatus", id: `${arg.sessionId}-${arg.provider}` },
      ],
    }),

    configureProvider: build.mutation<
      ConfigureProviderResponse,
      MessageInitShape<typeof ConfigureProviderRequestSchema>
    >({
      query: (input) => ({
        service: SessionService,
        method: "configureProvider",
        input,
      }),
      invalidatesTags: (_result, _error, arg) => [
        { type: "SessionProviderStatus", id: arg.sessionId },
        { type: "SessionProviderStatus", id: `${arg.sessionId}-${arg.provider}` },
      ],
    }),

    unconfigureProvider: build.mutation<
      UnconfigureProviderResponse,
      MessageInitShape<typeof UnconfigureProviderRequestSchema>
    >({
      query: (input) => ({
        service: SessionService,
        method: "unconfigureProvider",
        input,
      }),
      invalidatesTags: (_result, _error, arg) => [
        { type: "SessionProviderStatus", id: arg.sessionId },
        { type: "SessionProviderStatus", id: `${arg.sessionId}-${arg.provider}` },
      ],
    }),
  }),
});

export const {
  useListConfiguredProvidersQuery,
  useGetProviderStatusQuery,
  useConfigureProviderMutation,
  useUnconfigureProviderMutation,
} = sessionProviderApi;
