import { createApi } from "@reduxjs/toolkit/query/react";
import { createGrpcBaseQuery } from "../api";
import type {
  CreateSessionResponse,
  DeleteSessionResponse,
  GetSessionResponse,
  ListSessionsResponse,
} from "@kf2/proto/gen/session_pb.js";
import {
  DeleteSessionRequestSchema,
  GetSessionRequestSchema,
  SessionService,
} from "@kf2/proto/gen/session_pb.js";
import type { MessageInitShape } from "@bufbuild/protobuf";

export const sessionApi = createApi({
  reducerPath: "sessionApi",
  baseQuery: createGrpcBaseQuery(),
  tagTypes: ["Session"],
  endpoints: (build) => ({
    listSessions: build.query<ListSessionsResponse, void>({
      query: () => ({
        service: SessionService,
        method: "listSessions",
        input: {},
      }),
      providesTags: ["Session"],
    }),

    getSession: build.query<
      GetSessionResponse,
      MessageInitShape<typeof GetSessionRequestSchema>
    >({
      query: (input) => ({
        service: SessionService,
        method: "getSession",
        input,
      }),
      providesTags: ["Session"],
    }),

    createSession: build.mutation<CreateSessionResponse, void>({
      query: () => ({
        service: SessionService,
        method: "createSession",
        input: {},
      }),
      invalidatesTags: ["Session"],
    }),

    deleteSession: build.mutation<
      DeleteSessionResponse,
      MessageInitShape<typeof DeleteSessionRequestSchema>
    >({
      query: (input) => ({
        service: SessionService,
        method: "deleteSession",
        input,
      }),
      invalidatesTags: ["Session"],
    }),
  }),
});

export const {
  useListSessionsQuery,
  useGetSessionQuery,
  useCreateSessionMutation,
  useDeleteSessionMutation,
} = sessionApi;
