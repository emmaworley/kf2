import { useListSessionsQuery } from "@kf2/common/store";

export function App() {
  const { data, isLoading } = useListSessionsQuery();

  return (
    <div>
      <h1>KF2 Projector</h1>
      {isLoading ? (
        <p>Connecting…</p>
      ) : (
        <p>Active sessions: {data?.sessions?.length ?? 0}</p>
      )}
    </div>
  );
}
