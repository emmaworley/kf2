import { createGrpcWebTransport } from "@connectrpc/connect-web";

/**
 * Shared gRPC-Web transport. The base URL points at the Rust server origin,
 * which is always the same origin in both dev (reverse-proxied) and prod.
 */
export const transport = createGrpcWebTransport({
  baseUrl: window.location.origin,
});
