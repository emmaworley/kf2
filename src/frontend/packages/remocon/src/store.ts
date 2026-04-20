import { configureStore } from "@reduxjs/toolkit";
import { sessionApi } from "@kf2/common/store";

export const store = configureStore({
  reducer: {
    [sessionApi.reducerPath]: sessionApi.reducer,
  },
  middleware: (getDefault) => getDefault().concat(sessionApi.middleware),
});

export type RootState = ReturnType<typeof store.getState>;
export type AppDispatch = typeof store.dispatch;
