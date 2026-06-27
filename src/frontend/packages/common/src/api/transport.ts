import { createGrpcWebTransport } from "@connectrpc/connect-web";

const API_BASE_URL =
  import.meta.env.VITE_SERVER_URL ?? window.location.origin;

export const transport = createGrpcWebTransport({
  baseUrl: API_BASE_URL,
});
