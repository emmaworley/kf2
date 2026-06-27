import { configureStore } from "@reduxjs/toolkit";
import { providerApi, sessionApi, sessionProviderApi } from "@kf2/common/store";

export const store = configureStore({
  reducer: {
    [sessionApi.reducerPath]: sessionApi.reducer,
    [providerApi.reducerPath]: providerApi.reducer,
    [sessionProviderApi.reducerPath]: sessionProviderApi.reducer,
  },
  middleware: (getDefault) =>
    getDefault().concat(
      sessionApi.middleware,
      providerApi.middleware,
      sessionProviderApi.middleware,
    ),
});

export type RootState = ReturnType<typeof store.getState>;
export type AppDispatch = typeof store.dispatch;
