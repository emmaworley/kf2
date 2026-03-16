import type { BaseQueryFn } from "@reduxjs/toolkit/query";
import type { Transport } from "@connectrpc/connect";
import { ConnectError, createClient } from "@connectrpc/connect";
import type { DescService } from "@bufbuild/protobuf";
import { transport as defaultTransport } from "./transport.js";

/**
 * Arguments passed to the custom baseQuery from each endpoint's `query` fn.
 */
export interface GrpcQueryArgs<I = unknown> {
  service: DescService;
  method: string;
  input: I;
}

/**
 * Error shape returned by the baseQuery on failure.
 */
export interface GrpcQueryError {
  code: number;
  message: string;
}

/**
 * Creates an RTK Query baseQuery that dispatches RPCs via Connect-ES.
 *
 * Each endpoint's `query` function returns a `GrpcQueryArgs` describing which
 * service, method, and input to use. The baseQuery creates a Connect client
 * and invokes the method.
 */
export function createGrpcBaseQuery(
  overrideTransport?: Transport,
): BaseQueryFn<GrpcQueryArgs, unknown, GrpcQueryError> {
  const t = overrideTransport ?? defaultTransport;

  return async (args) => {
    try {
      const client = createClient(args.service, t);
      const fn = (client as Record<string, Function>)[args.method];
      if (!fn) {
        return {
          error: { code: 12, message: `Unknown method: ${args.method}` },
        };
      }
      const data = await fn.call(client, args.input);
      return { data };
    } catch (err) {
      if (err instanceof ConnectError) {
        return {
          error: { code: err.code, message: err.message },
        };
      }
      return {
        error: {
          code: 2,
          message: err instanceof Error ? err.message : "Unknown error",
        },
      };
    }
  };
}
